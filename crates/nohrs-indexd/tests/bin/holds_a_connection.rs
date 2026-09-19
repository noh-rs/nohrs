//! Holds a connection to the daemon open and does nothing else, so that a test
//! can kill it outright and see whether the daemon notices.
//!
//! A process rather than a thread on purpose: what is being tested is that the
//! kernel closes the socket when the holder dies, which no amount of Rust can
//! arrange from inside the process being killed.

fn main() -> std::process::ExitCode {
    let Some(socket) = std::env::args_os().nth(1) else {
        eprintln!("usage: holds_a_connection <socket>");
        return std::process::ExitCode::FAILURE;
    };

    #[cfg(unix)]
    {
        use std::os::unix::net::UnixStream;

        use nohrs_indexd::protocol::{Request, VERSION, write_frame};

        let mut stream = match UnixStream::connect(&socket) {
            Ok(stream) => stream,
            Err(error) => {
                eprintln!("cannot connect: {error}");
                return std::process::ExitCode::FAILURE;
            }
        };
        // Greeted through the protocol rather than by hand, so a change to it
        // cannot leave this helper silently failing to connect — which would
        // make the test that kills it pass for the wrong reason.
        if let Err(error) = write_frame(&mut stream, &Request::Hello { version: VERSION }) {
            eprintln!("cannot greet: {error:#}");
            return std::process::ExitCode::FAILURE;
        }
        println!("connected");
        // Sleeps until killed. Far longer than any test's patience.
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
    std::process::ExitCode::SUCCESS
}
