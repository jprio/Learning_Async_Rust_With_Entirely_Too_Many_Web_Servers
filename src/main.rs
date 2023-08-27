use std::collections::HashMap;
use std::net::TcpListener;
use chrono::{ DateTime, Utc };
use std::io;
use std::io::Read;
use std::io::Write;
use std::os::windows::io::AsRawSocket;
use polling::{ Event, Poller };

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
    let epoll = Poller::new().unwrap();

    let listener = TcpListener::bind("localhost:3000").unwrap();
    listener.set_nonblocking(true).unwrap();
    let listener_fd = listener.as_raw_socket() as usize;
    //epoll.add(&listener, Event::readable(key)).unwrap();
    epoll.add(&listener, Event::all(listener_fd)).unwrap();

    let mut connections = HashMap::new();
    let mut events = Vec::new();

    loop {
        // Wait for at least one I/O event.
        events.clear();
        epoll.wait(&mut events, Some(std::time::Duration::from_millis(1000))).unwrap();
        let mut completed = Vec::new();

        'next: for ev in &events {
            let fd = ev.key;

            if fd == listener_fd {
                info!("listener event");
                // Perform a non-blocking accept operation.
                match listener.accept() {
                    Ok((connection, _)) => {
                        info!("New connection accepted");
                        connection.set_nonblocking(true).unwrap();
                        epoll
                            .add(&connection, Event::all(connection.as_raw_socket() as usize))
                            .unwrap();
                        let state = ConnectionState::Read {
                            request: [0u8; 1024],
                            read: 0,
                        };
                        info!("inset connection {} in connections", connection.as_raw_socket());
                        connections.insert(connection.as_raw_socket() as usize, (
                            connection,
                            state,
                        ));
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(e) => panic!("{e}"),
                }

                // Set interest in the next readability event.
                epoll.modify(&listener, Event::readable(listener_fd)).unwrap();
                continue 'next;
            }
            // otherwise, a connection is ready
            info!("get connection {}", fd);
            let (connection, state) = connections.get_mut(&fd).unwrap();

            if let ConnectionState::Read { request, read } = state {
                loop {
                    // try reading from the stream
                    match connection.read(&mut request[*read..]) {
                        Ok(0) => {
                            println!("client disconnected unexpectedly");
                            completed.push(fd);
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
                            completed.push(fd);
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
                        completed.push(fd);
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        // not ready yet, move on to the next connection
                        continue 'next;
                    }
                    Err(e) => panic!("{e}"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_add() {}
}
