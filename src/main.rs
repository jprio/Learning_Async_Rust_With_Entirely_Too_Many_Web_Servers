use std::net::TcpListener;
use chrono::{ DateTime, Utc };
use std::io;
use std::io::Read;
use std::io::Write;

use log::{ info, warn, error };

enum ConnectionState {
    Read {
        request: [u8; 1024],
        read: usize,
    },
    Write {
        response: &'static [u8],
        written: usize,
    },
    Flush,
}
fn main() {
    env_logger::init();
    let listener = TcpListener::bind("localhost:3000").unwrap();
    let mut connections = Vec::new();

    loop {
        match listener.accept() {
            Ok((connection, _)) => {
                info!("New connection accepted");
                connection.set_nonblocking(true).unwrap();
                let state = ConnectionState::Read {
                    request: [0u8; 1024],
                    read: 0,
                };

                connections.push((connection, state));
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => panic!("{e}"),
        }

        let mut completed: Vec<usize> = Vec::new();
        'next: for (i, (connection, state)) in connections.iter_mut().enumerate() {
            info!("Handling conn {}", i);
            if let ConnectionState::Read { request, read } = state {
                loop {
                    // try reading from the stream
                    match connection.read(&mut request[*read..]) {
                        Ok(0) => {
                            println!("client disconnected unexpectedly");
                            completed.push(i);
                            continue 'next;
                        }
                        Ok(n) => {
                            // keep track of how many bytes we've read
                            *read += n;
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            // not ready yet, move on to the next connection
                            continue 'next;
                        }
                        Err(e) => panic!("{e}"),
                    }

                    // did we reach the end of the request?
                    if request.get(*read - 4..*read) == Some(b"\r\n\r\n") {
                        break;
                    }
                }

                // we're done, print the request
                let request = String::from_utf8_lossy(&request[..*read]);
                println!("{request}");
                // move into the write state
                let response = concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "Content-Length: 12\n",
                    "Connection: close\r\n\r\n",
                    "Hello world!"
                );
                *state = ConnectionState::Write { // 👈
                    response: response.as_bytes(),
                    written: 0,
                };
            }

            if let ConnectionState::Write { response, written } = state {
                loop {
                    match connection.write(&response[*written..]) {
                        Ok(0) => {
                            println!("client disconnected unexpectedly");
                            completed.push(i);
                            continue 'next;
                        }
                        Ok(n) => {
                            *written += n;
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            // not ready yet, move on to the next connection
                            continue 'next;
                        }
                        Err(e) => panic!("{e}"),
                    }

                    // did we write the whole response yet?
                    if *written == response.len() {
                        break;
                    }
                }

                // successfully wrote the response, try flushing next
                *state = ConnectionState::Flush;
            }
            if let ConnectionState::Flush = state {
                match connection.flush() {
                    Ok(_) => {
                        completed.push(i);
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        // not ready yet, move on to the next connection
                        continue 'next;
                    }
                    Err(e) => panic!("{e}"),
                }
            }
        }
        for i in completed.into_iter().rev() {
            connections.remove(i);
        }
    }
}

fn handle_connection(mut connection: std::net::TcpStream) -> io::Result<()> {
    let now: DateTime<Utc> = Utc::now();
    let mut request = [0u8; 1024];
    let mut read = 0;
    info!("{:?} : handle connection", now);
    loop {
        // try reading from the stream
        let num_bytes = connection.read(&mut request[read..])?;

        // the client disconnected
        if num_bytes == 0 {
            println!("client disconnected unexpectedly");
            return Ok(());
        }

        // keep track of how many bytes we've read
        read += num_bytes;
        // have we reached the end of the request?
        if request.get(read - 4..read) == Some(b"\r\n\r\n") {
            break;
        }
    }
    let request = String::from_utf8_lossy(&request[..read]);
    info!("{request}");
    // "Hello World!" in HTTP
    let response = concat!(
        "HTTP/1.1 200 OK\r\n",
        "Content-Length: 12\n",
        "Connection: close\r\n\r\n",
        "Hello world!"
    );

    let mut written = 0;

    loop {
        // write the remaining response bytes
        let num_bytes = connection.write(response[written..].as_bytes())?;

        // the client disconnected
        if num_bytes == 0 {
            println!("client disconnected unexpectedly");
            return Ok(());
        }

        written += num_bytes;

        // have we written the whole response yet?
        if written == response.len() {
            break;
        }
    }
    // flush the response
    connection.flush();

    Ok(())
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_add() {}
}
