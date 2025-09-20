use std::error::Error;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use threadpool::ThreadPool;

/// Handle a single connection: read up to a limit, then write a simple HTTP response and close.
fn handle_connection(mut stream: TcpStream, peer: SocketAddr) {
    println!("Handling connection from {}", peer);

    // Read a small amount of the request (we don't fully parse HTTP here)
    let mut buf = [0u8; 4096];
    match stream.read(&mut buf) {
        Ok(0) => {
            println!("{}: client closed connection immediately", peer);
            return;
        }
        Ok(n) => {
            // For debugging: print the request (as text if valid UTF-8)
            if let Ok(req_str) = std::str::from_utf8(&buf[..n]) {
                println!("{} sent request:\n{}", peer, req_str);
            } else {
                println!("{} sent {} bytes (non-UTF8)", peer, n);
            }
        }
        Err(e) => {
            eprintln!("{}: read error: {}", peer, e);
            return;
        }
    }

    // Prepare a simple body and response
    let body = "Hello from Rust threadpool server!\n";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{}",
        body.len(),
        body
    );

    // Write the response
    if let Err(e) = stream.write_all(response.as_bytes()) {
        eprintln!("{}: write error: {}", peer, e);
    }

    // Ensure data is flushed before we drop the stream
    if let Err(e) = stream.flush() {
        eprintln!("{}: flush error: {}", peer, e);
    }

    println!("{}: response sent, closing", peer);
}

fn main() -> Result<(), Box<dyn Error>> {
    // Bind to localhost:8000
    let listener = TcpListener::bind(("127.0.0.1", 8000))?;
    println!("Listening on {}", listener.local_addr()?);

    // Create a thread pool with 8 workers
    let pool = ThreadPool::new(8);

    // Shared state placeholder (if you need it later)
    let _shared_state = Arc::new(());

    for stream_res in listener.incoming() {
        match stream_res {
            Ok(stream) => {
                let peer: SocketAddr = match stream.peer_addr() {
                    Ok(p) => p,
                    Err(_) => {
                        eprintln!("Failed to get peer addr; dropping connection");
                        continue;
                    }
                };

                // Clone what the worker needs (none in this example)
                let _state = Arc::clone(&_shared_state);

                pool.execute(move || {
                    handle_connection(stream, peer);
                });
            }
            Err(e) => eprintln!("Accept error: {}", e),
        }
    }

    Ok(())
}
