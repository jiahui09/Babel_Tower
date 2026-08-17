use std::{
    ffi::OsStr,
    io::{self, Read},
    process::{Child, ChildStdin, Command, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use thiserror::Error;
use uuid::Uuid;

use crate::ipc::{
    Handshake, PROTOCOL_MAJOR, PROTOCOL_MINOR, ProtocolError, WorkerRequest, WorkerResponse,
    read_frame, validate_handshake, write_frame,
};

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("failed to spawn worker: {0}")]
    Spawn(#[source] io::Error),
    #[error("worker stdin is unavailable")]
    MissingStdin,
    #[error("worker stdout is unavailable")]
    MissingStdout,
    #[error("worker stderr is unavailable")]
    MissingStderr,
    #[error("worker handshake failed: {0}")]
    Handshake(#[source] ProtocolError),
    #[error("worker request write failed: {0}")]
    RequestWrite(#[source] ProtocolError),
    #[error("worker response failed: {0}")]
    Response(#[source] ProtocolError),
    #[error("worker response request_id {actual} did not match expected {expected}")]
    RequestIdMismatch { expected: u64, actual: u64 },
    #[error("worker response payload {actual} exceeds the {maximum} byte limit")]
    ResponseTooLarge { actual: usize, maximum: usize },
    #[error("worker did not complete {phase} within {timeout:?}")]
    Timeout {
        phase: &'static str,
        timeout: Duration,
        diagnostic: WorkerExitDiagnostic,
    },
    #[error("worker was cancelled during {phase}")]
    Cancelled {
        phase: &'static str,
        diagnostic: WorkerExitDiagnostic,
    },
    #[error("worker exited before {phase} completed: {diagnostic}")]
    WorkerExited {
        phase: &'static str,
        diagnostic: WorkerExitDiagnostic,
    },
    #[error("worker stdout reader terminated unexpectedly")]
    ReaderThreadFailed,
    #[error("worker kill failed: {0}")]
    Kill(#[source] io::Error),
    #[error("worker wait failed: {0}")]
    Wait(#[source] io::Error),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerExitDiagnostic {
    pub status: Option<String>,
    pub stderr: String,
}

impl std::fmt::Display for WorkerExitDiagnostic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.status, self.stderr.trim().is_empty()) {
            (Some(status), true) => write!(formatter, "status={status}"),
            (Some(status), false) => write!(formatter, "status={status}; stderr={}", self.stderr),
            (None, true) => write!(formatter, "status=<unknown>"),
            (None, false) => write!(formatter, "status=<unknown>; stderr={}", self.stderr),
        }
    }
}

#[derive(Clone, Debug)]
pub struct WorkerLaunch {
    pub program: std::path::PathBuf,
    pub args: Vec<std::ffi::OsString>,
    pub capability_token: Vec<u8>,
    pub handshake_timeout: Duration,
    pub request_timeout: Duration,
    pub max_response_bytes: usize,
}

impl WorkerLaunch {
    pub fn new(program: impl Into<std::path::PathBuf>, capability_token: Vec<u8>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            capability_token,
            handshake_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(30),
            max_response_bytes: crate::ipc::MAX_FRAME_BYTES,
        }
    }

    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.args.push(arg.as_ref().to_owned());
        self
    }
}

#[derive(Clone, Debug, Default)]
pub struct WorkerCancelToken {
    cancelled: Arc<AtomicBool>,
}

impl WorkerCancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

enum ReaderEvent {
    Handshake(Result<Handshake, ProtocolError>),
    Response(Result<WorkerResponse, ProtocolError>),
}

pub struct ProcessWorker {
    child: Child,
    stdin: Option<ChildStdin>,
    events: Receiver<ReaderEvent>,
    reader: Option<JoinHandle<()>>,
    session_nonce: Vec<u8>,
    capability_token: Vec<u8>,
    max_response_bytes: usize,
    request_timeout: Duration,
    stderr: Option<std::process::ChildStderr>,
}

impl ProcessWorker {
    pub fn spawn(launch: WorkerLaunch, cancel: &WorkerCancelToken) -> Result<Self, WorkerError> {
        let mut command = Command::new(&launch.program);
        command
            .args(&launch.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(WorkerError::Spawn)?;
        let mut stdin = child.stdin.take().ok_or(WorkerError::MissingStdin)?;
        let stdout = child.stdout.take().ok_or(WorkerError::MissingStdout)?;
        let stderr = child.stderr.take().ok_or(WorkerError::MissingStderr)?;

        let session_nonce = Uuid::new_v4().as_bytes().to_vec();
        let handshake = Handshake {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            session_nonce: session_nonce.clone(),
            capability_token: launch.capability_token.clone(),
        };
        write_frame(&mut stdin, &handshake).map_err(WorkerError::Handshake)?;

        let (sender, events) = mpsc::channel();
        let reader = thread::spawn(move || {
            let mut stdout = stdout;
            let first = read_frame::<_, Handshake>(&mut stdout);
            if sender.send(ReaderEvent::Handshake(first)).is_err() {
                return;
            }
            loop {
                let response = read_frame::<_, WorkerResponse>(&mut stdout);
                let should_continue = response.is_ok();
                if sender.send(ReaderEvent::Response(response)).is_err() || !should_continue {
                    return;
                }
            }
        });

        let mut worker = Self {
            child,
            stdin: Some(stdin),
            events,
            reader: Some(reader),
            session_nonce,
            capability_token: launch.capability_token,
            max_response_bytes: launch.max_response_bytes,
            request_timeout: launch.request_timeout,
            stderr: Some(stderr),
        };

        match worker.wait_for_event("handshake", launch.handshake_timeout, cancel)? {
            ReaderEvent::Handshake(Ok(ack)) => {
                validate_handshake(&ack, &worker.session_nonce, &worker.capability_token)
                    .map_err(WorkerError::Handshake)?;
                Ok(worker)
            }
            ReaderEvent::Handshake(Err(error)) => Err(worker.exit_error("handshake", error)),
            ReaderEvent::Response(_) => {
                let diagnostic = worker.terminate();
                Err(WorkerError::WorkerExited {
                    phase: "handshake",
                    diagnostic,
                })
            }
        }
    }

    pub fn request(
        &mut self,
        request_id: u64,
        payload: Vec<u8>,
        cancel: &WorkerCancelToken,
    ) -> Result<WorkerResponse, WorkerError> {
        self.request_with_timeout(request_id, payload, self.request_timeout, cancel)
    }

    pub fn request_with_timeout(
        &mut self,
        request_id: u64,
        payload: Vec<u8>,
        timeout: Duration,
        cancel: &WorkerCancelToken,
    ) -> Result<WorkerResponse, WorkerError> {
        let stdin = self.stdin.as_mut().ok_or(WorkerError::MissingStdin)?;
        write_frame(
            stdin,
            &WorkerRequest {
                request_id,
                payload,
            },
        )
        .map_err(WorkerError::RequestWrite)?;

        match self.wait_for_event("request", timeout, cancel)? {
            ReaderEvent::Response(Ok(response)) => {
                if response.request_id != request_id {
                    let actual = response.request_id;
                    let _ = self.terminate();
                    return Err(WorkerError::RequestIdMismatch {
                        expected: request_id,
                        actual,
                    });
                }
                if response.payload.len() > self.max_response_bytes {
                    let actual = response.payload.len();
                    let _ = self.terminate();
                    return Err(WorkerError::ResponseTooLarge {
                        actual,
                        maximum: self.max_response_bytes,
                    });
                }
                Ok(response)
            }
            ReaderEvent::Response(Err(error)) => Err(self.exit_error("request", error)),
            ReaderEvent::Handshake(_) => {
                let diagnostic = self.terminate();
                Err(WorkerError::WorkerExited {
                    phase: "request",
                    diagnostic,
                })
            }
        }
    }

    pub fn terminate(&mut self) -> WorkerExitDiagnostic {
        if matches!(self.child.try_wait(), Ok(None)) {
            self.stdin.take();
            let deadline = Instant::now() + Duration::from_millis(100);
            while Instant::now() < deadline {
                if !matches!(self.child.try_wait(), Ok(None)) {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
            if matches!(self.child.try_wait(), Ok(None)) {
                let _ = self.child.kill();
            }
        }
        self.wait_and_collect()
            .unwrap_or_else(|error| WorkerExitDiagnostic {
                status: Some(format!("wait error: {error}")),
                stderr: String::new(),
            })
    }

    fn wait_for_event(
        &mut self,
        phase: &'static str,
        timeout: Duration,
        cancel: &WorkerCancelToken,
    ) -> Result<ReaderEvent, WorkerError> {
        let started = Instant::now();
        loop {
            if cancel.is_cancelled() {
                let diagnostic = self.terminate();
                return Err(WorkerError::Cancelled { phase, diagnostic });
            }
            let remaining = match timeout.checked_sub(started.elapsed()) {
                Some(remaining) => remaining,
                None => {
                    let diagnostic = self.terminate();
                    return Err(WorkerError::Timeout {
                        phase,
                        timeout,
                        diagnostic,
                    });
                }
            };
            let poll = remaining.min(Duration::from_millis(25));
            match self.events.recv_timeout(poll) {
                Ok(event) => return Ok(event),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let diagnostic = self.terminate();
                    return Err(WorkerError::WorkerExited { phase, diagnostic });
                }
            }
        }
    }

    fn exit_error(&mut self, phase: &'static str, error: ProtocolError) -> WorkerError {
        match error {
            ProtocolError::Io(io_error) if io_error.kind() == io::ErrorKind::UnexpectedEof => {
                let diagnostic = self.terminate();
                WorkerError::WorkerExited { phase, diagnostic }
            }
            error if phase == "handshake" => WorkerError::Handshake(error),
            error => WorkerError::Response(error),
        }
    }

    fn wait_and_collect(&mut self) -> io::Result<WorkerExitDiagnostic> {
        let status = self.child.wait()?;
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        let mut stderr = String::new();
        if let Some(mut pipe) = self.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr);
        }
        Ok(WorkerExitDiagnostic {
            status: Some(format_status(status)),
            stderr,
        })
    }
}

impl Drop for ProcessWorker {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}

fn format_status(status: ExitStatus) -> String {
    status
        .code()
        .map(|code| format!("exit code {code}"))
        .unwrap_or_else(|| status.to_string())
}

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        process::Command,
        sync::OnceLock,
        time::{Duration, Instant},
    };

    use super::*;

    static FIXTURE: OnceLock<PathBuf> = OnceLock::new();

    fn fixture_binary() -> &'static PathBuf {
        FIXTURE.get_or_init(|| {
            let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
            let manifest = manifest_dir.join("../../tools/worker-fixture/Cargo.toml");
            let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
            let status = Command::new(cargo)
                .arg("build")
                .arg("--quiet")
                .arg("--manifest-path")
                .arg(&manifest)
                .status()
                .expect("build worker fixture");
            assert!(status.success(), "worker fixture build failed: {status}");

            let mut binary =
                manifest_dir.join("../../tools/worker-fixture/target/debug/worker-fixture");
            if cfg!(windows) {
                binary.set_extension("exe");
            }
            assert!(binary.exists(), "worker fixture binary was not created");
            binary
        })
    }

    fn launch(mode: &str) -> WorkerLaunch {
        let mut launch =
            WorkerLaunch::new(fixture_binary(), b"fixture-capability".to_vec()).arg(mode);
        launch.handshake_timeout = Duration::from_secs(2);
        launch.request_timeout = Duration::from_secs(2);
        launch
    }

    #[test]
    fn worker_echoes_single_correlated_request() {
        let cancel = WorkerCancelToken::new();
        let mut worker = ProcessWorker::spawn(launch("echo"), &cancel).unwrap();
        let response = worker.request(41, b"payload".to_vec(), &cancel).unwrap();
        assert_eq!(response.request_id, 41);
        assert_eq!(response.payload, b"payload");
    }

    #[test]
    fn worker_terminate_closes_stdin_before_forced_kill() {
        let cancel = WorkerCancelToken::new();
        let mut worker = ProcessWorker::spawn(launch("echo"), &cancel).unwrap();
        let diagnostic = worker.terminate();
        assert_eq!(diagnostic.status.as_deref(), Some("exit code 0"));
    }

    #[test]
    fn worker_timeout_kills_and_waits() {
        let cancel = WorkerCancelToken::new();
        let mut worker = ProcessWorker::spawn(launch("timeout"), &cancel).unwrap();
        let started = Instant::now();
        let error = worker
            .request_with_timeout(1, Vec::new(), Duration::from_millis(100), &cancel)
            .unwrap_err();
        assert!(matches!(error, WorkerError::Timeout { .. }));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn worker_cancellation_kills_and_waits() {
        let cancel = WorkerCancelToken::new();
        let mut worker = ProcessWorker::spawn(launch("timeout"), &cancel).unwrap();
        cancel.cancel();
        let error = worker
            .request_with_timeout(1, Vec::new(), Duration::from_secs(30), &cancel)
            .unwrap_err();
        assert!(matches!(error, WorkerError::Cancelled { .. }));
    }

    #[test]
    fn worker_crash_reports_stderr_and_status() {
        let cancel = WorkerCancelToken::new();
        let error = match ProcessWorker::spawn(launch("crash-before-handshake"), &cancel) {
            Ok(_) => panic!("expected worker spawn to fail"),
            Err(error) => error,
        };
        match error {
            WorkerError::WorkerExited { diagnostic, .. } => {
                assert!(
                    diagnostic
                        .stderr
                        .contains("fixture crashed before handshake")
                );
                assert!(diagnostic.status.unwrap().contains("17"));
            }
            other => panic!("expected worker exit, got {other:?}"),
        }
    }

    #[test]
    fn worker_rejects_bad_ack_nonce_and_version() {
        let cancel = WorkerCancelToken::new();
        let nonce_error = match ProcessWorker::spawn(launch("wrong-nonce"), &cancel) {
            Ok(_) => panic!("expected worker spawn to fail"),
            Err(error) => error,
        };
        assert!(matches!(
            nonce_error,
            WorkerError::Handshake(ProtocolError::InvalidNonce)
        ));

        let version_error = match ProcessWorker::spawn(launch("wrong-version"), &cancel) {
            Ok(_) => panic!("expected worker spawn to fail"),
            Err(error) => error,
        };
        assert!(matches!(
            version_error,
            WorkerError::Handshake(ProtocolError::IncompatibleMajor { .. })
        ));
    }

    #[test]
    fn worker_rejects_wrong_request_id_and_oversized_response() {
        let cancel = WorkerCancelToken::new();
        let mut worker = ProcessWorker::spawn(launch("wrong-request-id"), &cancel).unwrap();
        let error = worker.request(9, Vec::new(), &cancel).unwrap_err();
        assert!(matches!(
            error,
            WorkerError::RequestIdMismatch {
                expected: 9,
                actual: 10
            }
        ));

        let mut small_response_limit = launch("big-response");
        small_response_limit.max_response_bytes = 4;
        let mut worker = ProcessWorker::spawn(small_response_limit, &cancel).unwrap();
        let error = worker.request(11, Vec::new(), &cancel).unwrap_err();
        assert!(matches!(error, WorkerError::ResponseTooLarge { .. }));
    }
}
