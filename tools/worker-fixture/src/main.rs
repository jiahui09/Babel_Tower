use std::{env, io, process, thread, time::Duration};

use babel_runtime::ipc::{
    Handshake, PROTOCOL_MAJOR, PROTOCOL_MINOR, WorkerRequest, WorkerResponse, read_frame,
    validate_handshake, write_frame,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mode = env::args().nth(1).unwrap_or_else(|| "echo".to_owned());
    if mode == "crash-before-handshake" {
        eprintln!("fixture crashed before handshake");
        process::exit(17);
    }

    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    let handshake: Handshake = read_frame(&mut stdin)?;
    validate_handshake(
        &handshake,
        &handshake.session_nonce,
        &handshake.capability_token,
    )?;

    match mode.as_str() {
        "wrong-nonce" => {
            let mut ack = handshake;
            ack.session_nonce = b"wrong".to_vec();
            write_frame(&mut stdout, &ack)?;
            return Ok(());
        }
        "wrong-version" => {
            let mut ack = handshake;
            ack.protocol_major = PROTOCOL_MAJOR + 1;
            write_frame(&mut stdout, &ack)?;
            return Ok(());
        }
        _ => {
            write_frame(
                &mut stdout,
                &Handshake {
                    protocol_major: PROTOCOL_MAJOR,
                    protocol_minor: PROTOCOL_MINOR,
                    session_nonce: handshake.session_nonce,
                    capability_token: handshake.capability_token,
                },
            )?;
        }
    }

    if mode == "timeout" {
        thread::sleep(Duration::from_secs(60));
        return Ok(());
    }
    if mode == "crash-after-handshake" {
        eprintln!("fixture crashed after handshake");
        process::exit(23);
    }

    loop {
        let request: WorkerRequest = match read_frame(&mut stdin) {
            Ok(request) => request,
            Err(_) => return Ok(()),
        };
        let mut response = WorkerResponse {
            request_id: request.request_id,
            status: 0,
            payload: request.payload,
            diagnostic: String::new(),
        };
        if mode == "big-response" {
            response.payload = vec![0x5a; 1024];
        }
        if mode == "wrong-request-id" {
            response.request_id += 1;
        }
        write_frame(&mut stdout, &response)?;
    }
}
