#[cfg(not(unix))]
use std::thread;
use std::{
    io::{self, Read, Write},
    time::{Duration, Instant},
};

#[cfg(not(unix))]
use interprocess::local_socket::{
    GenericNamespaced, ListenerOptions, Stream, ToNsName, prelude::*,
};
use prost::Message;
#[cfg(unix)]
use std::process::{Command, Stdio};
use thiserror::Error;
use uuid::Uuid;

pub const PROTOCOL_MAJOR: u32 = 1;
pub const PROTOCOL_MINOR: u32 = 1;
pub const MIN_SUPPORTED_MINOR: u32 = 0;
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, PartialEq, Message)]
pub struct Handshake {
    #[prost(uint32, tag = "1")]
    pub protocol_major: u32,
    #[prost(uint32, tag = "2")]
    pub protocol_minor: u32,
    #[prost(bytes = "vec", tag = "3")]
    pub session_nonce: Vec<u8>,
    #[prost(bytes = "vec", tag = "4")]
    pub capability_token: Vec<u8>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ProbeRequest {
    #[prost(uint64, tag = "1")]
    pub request_id: u64,
    #[prost(bytes = "vec", tag = "2")]
    pub payload: Vec<u8>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ProbeResponse {
    #[prost(uint64, tag = "1")]
    pub request_id: u64,
    #[prost(uint32, tag = "2")]
    pub accepted_bytes: u32,
}

#[derive(Clone, PartialEq, Message)]
pub struct WorkerRequest {
    #[prost(uint64, tag = "1")]
    pub request_id: u64,
    #[prost(bytes = "vec", tag = "2")]
    pub payload: Vec<u8>,
}

#[derive(Clone, PartialEq, Message)]
pub struct WorkerResponse {
    #[prost(uint64, tag = "1")]
    pub request_id: u64,
    #[prost(uint32, tag = "2")]
    pub status: u32,
    #[prost(bytes = "vec", tag = "3")]
    pub payload: Vec<u8>,
    #[prost(string, tag = "4")]
    pub diagnostic: String,
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("frame size {actual} exceeds the {maximum} byte limit")]
    FrameTooLarge { actual: usize, maximum: usize },
    #[error("invalid protobuf: {0}")]
    Decode(#[from] prost::DecodeError),
    #[error("protocol major {actual} is incompatible with {expected}")]
    IncompatibleMajor { actual: u32, expected: u32 },
    #[error("protocol minor {actual} is outside the supported range {minimum}..={maximum}")]
    IncompatibleMinor {
        actual: u32,
        minimum: u32,
        maximum: u32,
    },
    #[error("session nonce does not match the endpoint")]
    InvalidNonce,
    #[error("capability token is invalid")]
    InvalidCapability,
    #[error("IPC peer returned an unexpected response")]
    UnexpectedResponse,
    #[error("IPC server thread terminated unexpectedly")]
    ServerThreadFailed,
}

pub fn write_frame<W: Write, M: Message>(writer: &mut W, message: &M) -> Result<(), ProtocolError> {
    let payload = message.encode_to_vec();
    if payload.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge {
            actual: payload.len(),
            maximum: MAX_FRAME_BYTES,
        });
    }
    writer.write_all(&(payload.len() as u32).to_be_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()?;
    Ok(())
}

pub fn read_frame<R: Read, M: Message + Default>(reader: &mut R) -> Result<M, ProtocolError> {
    let mut length = [0_u8; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge {
            actual: length,
            maximum: MAX_FRAME_BYTES,
        });
    }
    let mut payload = vec![0; length];
    reader.read_exact(&mut payload)?;
    Ok(M::decode(payload.as_slice())?)
}

pub fn validate_handshake(
    handshake: &Handshake,
    expected_nonce: &[u8],
    expected_capability: &[u8],
) -> Result<(), ProtocolError> {
    if handshake.protocol_major != PROTOCOL_MAJOR {
        return Err(ProtocolError::IncompatibleMajor {
            actual: handshake.protocol_major,
            expected: PROTOCOL_MAJOR,
        });
    }
    if !(MIN_SUPPORTED_MINOR..=PROTOCOL_MINOR).contains(&handshake.protocol_minor) {
        return Err(ProtocolError::IncompatibleMinor {
            actual: handshake.protocol_minor,
            minimum: MIN_SUPPORTED_MINOR,
            maximum: PROTOCOL_MINOR,
        });
    }
    if handshake.session_nonce != expected_nonce {
        return Err(ProtocolError::InvalidNonce);
    }
    if handshake.capability_token != expected_capability {
        return Err(ProtocolError::InvalidCapability);
    }
    Ok(())
}

pub fn run_local_probe(roundtrips: usize, payload_bytes: usize) -> Result<Duration, ProtocolError> {
    run_local_probe_impl(roundtrips, payload_bytes)
}

#[cfg(unix)]
fn run_local_probe_impl(
    roundtrips: usize,
    payload_bytes: usize,
) -> Result<Duration, ProtocolError> {
    let mut child = Command::new("cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let mut writer = child
        .stdin
        .take()
        .ok_or(ProtocolError::UnexpectedResponse)?;
    let mut reader = child
        .stdout
        .take()
        .ok_or(ProtocolError::UnexpectedResponse)?;
    let handshake = Handshake {
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: PROTOCOL_MINOR,
        session_nonce: Uuid::new_v4().as_bytes().to_vec(),
        capability_token: Uuid::new_v4().as_bytes().to_vec(),
    };
    write_frame(&mut writer, &handshake)?;
    let echoed_handshake: Handshake = read_frame(&mut reader)?;
    validate_handshake(
        &echoed_handshake,
        &handshake.session_nonce,
        &handshake.capability_token,
    )?;
    let payload = vec![0x5a; payload_bytes];
    let started = Instant::now();
    for request_id in 0..roundtrips as u64 {
        let request = ProbeRequest {
            request_id,
            payload: payload.clone(),
        };
        write_frame(&mut writer, &request)?;
        let echoed: ProbeRequest = read_frame(&mut reader)?;
        if echoed != request {
            return Err(ProtocolError::UnexpectedResponse);
        }
    }
    let elapsed = started.elapsed();
    drop(writer);
    if !child.wait()?.success() {
        return Err(ProtocolError::UnexpectedResponse);
    }
    Ok(elapsed)
}

#[cfg(not(unix))]
fn run_local_probe_impl(
    roundtrips: usize,
    payload_bytes: usize,
) -> Result<Duration, ProtocolError> {
    let socket_name = format!("babel-phase0-{}", Uuid::new_v4());
    let name = socket_name.to_ns_name::<GenericNamespaced>()?;
    let listener = ListenerOptions::new().name(name.clone()).create_sync()?;
    let nonce = Uuid::new_v4().as_bytes().to_vec();
    let capability = Uuid::new_v4().as_bytes().to_vec();
    let server_nonce = nonce.clone();
    let server_capability = capability.clone();

    let server = thread::spawn(move || -> Result<(), ProtocolError> {
        let mut connection = listener.accept()?;
        let handshake: Handshake = read_frame(&mut connection)?;
        validate_handshake(&handshake, &server_nonce, &server_capability)?;
        for _ in 0..roundtrips {
            let request: ProbeRequest = read_frame(&mut connection)?;
            write_frame(
                &mut connection,
                &ProbeResponse {
                    request_id: request.request_id,
                    accepted_bytes: request.payload.len() as u32,
                },
            )?;
        }
        Ok(())
    });

    let mut client = Stream::connect(name)?;
    write_frame(
        &mut client,
        &Handshake {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            session_nonce: nonce,
            capability_token: capability,
        },
    )?;
    let payload = vec![0x5a; payload_bytes];
    let started = Instant::now();
    for request_id in 0..roundtrips as u64 {
        write_frame(
            &mut client,
            &ProbeRequest {
                request_id,
                payload: payload.clone(),
            },
        )?;
        let response: ProbeResponse = read_frame(&mut client)?;
        if response.request_id != request_id || response.accepted_bytes != payload_bytes as u32 {
            return Err(ProtocolError::UnexpectedResponse);
        }
    }
    let elapsed = started.elapsed();
    server
        .join()
        .map_err(|_| ProtocolError::ServerThreadFailed)??;
    Ok(elapsed)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    fn valid_handshake() -> Handshake {
        Handshake {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            session_nonce: b"session-1".to_vec(),
            capability_token: b"project-read".to_vec(),
        }
    }

    #[test]
    fn frame_round_trip_is_length_delimited() {
        let request = ProbeRequest {
            request_id: 42,
            payload: b"hello".to_vec(),
        };
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &request).unwrap();
        let decoded: ProbeRequest = read_frame(&mut Cursor::new(bytes)).unwrap();
        assert_eq!(decoded, request);
    }

    #[test]
    fn oversized_frame_is_rejected_before_allocation() {
        let bytes = ((MAX_FRAME_BYTES + 1) as u32).to_be_bytes();
        let error = read_frame::<_, ProbeRequest>(&mut Cursor::new(bytes)).unwrap_err();
        assert!(matches!(error, ProtocolError::FrameTooLarge { .. }));
    }

    #[test]
    fn handshake_rejects_wrong_version_nonce_and_capability() {
        let mut handshake = valid_handshake();
        handshake.protocol_major += 1;
        assert!(matches!(
            validate_handshake(&handshake, b"session-1", b"project-read"),
            Err(ProtocolError::IncompatibleMajor { .. })
        ));

        let handshake = valid_handshake();
        assert!(matches!(
            validate_handshake(&handshake, b"wrong", b"project-read"),
            Err(ProtocolError::InvalidNonce)
        ));
        assert!(matches!(
            validate_handshake(&handshake, b"session-1", b"wrong"),
            Err(ProtocolError::InvalidCapability)
        ));
    }

    #[test]
    fn protobuf_probe_crosses_platform_local_ipc() {
        run_local_probe(1, 1024).unwrap();
    }

    #[test]
    fn reusable_local_probe_completes_multiple_roundtrips() {
        let elapsed = run_local_probe(20, 1024).unwrap();
        assert!(elapsed < Duration::from_secs(5));
    }
}
