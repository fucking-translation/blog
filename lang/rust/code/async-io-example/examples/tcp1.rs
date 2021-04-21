use mio::{Token, Poll, Interest, Events};
use failure::Error;
use mio::net::{TcpListener, TcpStream};
use std::time::{Instant, Duration};
use std::io::{Write, Read};

const SERVER_ACCEPT: Token = Token(0);
const SERVER: Token = Token(1);
const CLIENT: Token = Token(2);
const SERVER_HELLO: &[u8] = b"PING";
const CLIENT_HELLO: &[u8] = b"PONG";

fn main() -> Result<(), Error> {
    let addr = "127.0.0.1:9000".parse().unwrap();

    // Setup the server socket
    let mut server = TcpListener::bind(addr)?;

    // Create a poll instance
    let mut poll = Poll::new()?;

    // Start listening for incoming connections
    poll.registry()
        .register(&mut server, SERVER_ACCEPT, Interest::READABLE)?;

    // Setup the client socket
    let mut client = TcpStream::connect(addr)?;
    let mut server_handler = None;

    poll.registry()
        .register(&mut client, CLIENT, Interest::READABLE.add(Interest::WRITABLE))?;

    let mut events = Events::with_capacity(1024);

    let start = Instant::now();
    let timeout = Duration::from_millis(10);
    'top: loop {
        poll.poll(&mut events, None)?;
        for event in events.iter() {
            if start.elapsed() >= timeout {
                break 'top
            }

            match event.token() {
                SERVER_ACCEPT => {
                    let (mut handler, addr) = server.accept()?;
                    println!("accept from addr: {}", &addr);
                    poll.registry()
                        .register(&mut handler, SERVER, Interest::READABLE.add(Interest::WRITABLE))?;
                    server_handler = Some(handler);
                }
                SERVER => {
                    if event.is_writable() {
                        if let Some(ref mut handler) = &mut server_handler {
                            match handler.write(SERVER_HELLO) {
                                Ok(_) => {
                                    println!("server wrote");
                                }
                                Err(ref err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                                    println!("");
                                    continue
                                },
                                err => {
                                    err?;
                                }
                            }
                        }
                    }
                    if event.is_readable() {
                        let mut hello = [0; 4];
                        if let Some(ref mut handler) = &mut server_handler {
                            match handler.read_exact(&mut hello) {
                                Ok(_) => {
                                    assert_eq!(CLIENT_HELLO, &hello);
                                    println!("server received");
                                }
                                Err(ref err) if err.kind() == std::io::ErrorKind::WouldBlock => continue,
                                err => {
                                    err?;
                                }
                            }
                        }
                    }
                },
                CLIENT => {
                    if event.is_writable() {
                        match client.write(CLIENT_HELLO) {
                            Ok(_) => {
                                println!("client wrote");
                            }
                            Err(ref err) if err.kind() == std::io::ErrorKind::WouldBlock => continue,
                            err => {
                                err?;
                            },
                        }
                    }
                    if event.is_readable() {
                        let mut hello = [0; 4];
                        match client.read_exact(&mut hello) {
                            Ok(_) => {
                                assert_eq!(SERVER_HELLO, &hello);
                                println!("client received");
                            }
                            Err(ref err) if err.kind() == std::io::ErrorKind::WouldBlock => continue,
                            err => {
                                err?;
                            }
                        }
                    }
                },
                _ => unreachable!()
            }
        }
    };
    Ok(())
}