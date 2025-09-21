// src/bin/main.rs

// dependencies
use gcra_rate_limiter::RateLimiter;
use std::error::Error;
use std::hash::Hash;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use threadpool::ThreadPool;

fn handle_allowed_request(stream: &mut TcpStream, peer: SocketAddr) {
    // Read the request (same as before)
    let mut buf = [0u8; 4096];
    match stream.read(&mut buf) {
        Ok(0) => {
            println!("{}: client closed connection immediately", peer);
            return;
        }
        Ok(n) => {
            if let Ok(req_str) = std::str::from_utf8(&buf[..n]) {
                println!("{} sent request:\n{}", peer, req_str);
            }
        }
        Err(e) => {
            eprintln!("{}: read error: {}", peer, e);
            return;
        }
    }

    // Send normal response
    let body = "Hello from Rust GCRA rate-limited server!\n";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );

    send_response(stream, peer, &response);
}

fn handle_rate_limited_request(stream: &mut TcpStream, peer: SocketAddr) {
    println!("{}: Rate limited!", peer);

    let body = "Rate limit exceeded. Please try again later.\n";
    let response = format!(
        "HTTP/1.1 429 Too Many Requests\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nConnection: close\r\nRetry-After: 1\r\n\r\n{}",
        body.len(),
        body
    );

    send_response(stream, peer, &response);
}

fn handle_error_response(stream: &mut TcpStream, peer: SocketAddr) {
    let body = "Internal server error\n";
    let response = format!(
        "HTTP/1.1 500 Internal Server Error\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );

    send_response(stream, peer, &response);
}

fn send_response(stream: &mut TcpStream, peer: SocketAddr, response: &str) {
    if let Err(e) = stream.write_all(response.as_bytes()) {
        eprintln!("{}: write error: {}", peer, e);
        return;
    }

    if let Err(e) = stream.flush() {
        eprintln!("{}: flush error: {}", peer, e);
    }

    println!("{}: response sent, closing", peer);
}

/// Handle a single connection: read up to a limit, then write a simple HTTP response and close.
fn handle_connection<T>(mut stream: TcpStream, peer: SocketAddr, limiter: Arc<RateLimiter<T>>)
where
    T: Hash + Eq + Clone + From<IpAddr>,
{
    println!("Handling connection from {}", peer);

    // Get current timestamp
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();

    // Use IP address as client ID
    let client_id = peer.ip();

    // Check rate limit
    match limiter.is_allowed(client_id.into(), current_time) {
        Ok(true) => {
            // Request allowed - proceed normally
            handle_allowed_request(&mut stream, peer);
        }
        Ok(false) => {
            // Request denied - return 429
            handle_rate_limited_request(&mut stream, peer);
        }
        Err(e) => {
            // Rate limiter error
            eprintln!("{}: Rate limiter error: {}", peer, e);
            handle_error_response(&mut stream, peer);
        }
    }

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

    // Create shared rate limiter - 5 requests per second, burst of 10
    let rate_limiter = Arc::new(RateLimiter::<IpAddr>::new(2.0, 0.0).unwrap());

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

                let limiter = Arc::clone(&rate_limiter);

                pool.execute(move || {
                    handle_connection(stream, peer, limiter);
                });
            }
            Err(e) => eprintln!("Accept error: {}", e),
        }
    }

    Ok(())
}
