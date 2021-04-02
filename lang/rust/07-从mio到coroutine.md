# Rust 异步 I/O：从 mio 到 coroutine

## 引言

2018 年接近尾声，`rust`团队勉强立住了异步`IO`的`flag`，`async`成为了关键字，`Pin`，`Future`，`Poll`和`await!`也进入了标准库。不过一直以来实际项目中用不到这套东西，所以也没有主动去了解过。

最近心血来潮想用`rust`写点东西，但并找不到比较能看的文档（可能是因为`rust`发展太快了，很多都过时了），最后参考[这篇文章](https://cafbit.com/post/tokio_internals/)和 `new tokio` ([romio](https://github.com/withoutboats/romio)) 写了几个`demo`，并基于`mio`在`coroutine`中实现了简陋的异步`IO`。

最终实现的 file-server 如下：

```rust
// examples/async-echo.rs

#![feature(async_await)]
#![feature(await_macro)]
#![feature(futures_api)]

#[macro_use]
extern crate log;

use asyncio::executor::{block_on, spawn, TcpListener, TcpStream};
use asyncio::fs_future::{read_to_string};
use failure::Error;

fn main() -> Result<(), Error> {
    env_logger::init();
    block_on(new_server())?
}

const CRLF: &[char] = &['\r', '\n'];

async fn new_server() -> Result<(), Error> {
    let mut listener = TcpListener::bind(&"127.0.0.1:7878".parse()?)?;
    info!("Listening on 127.0.0.1:7878");
    while let Ok((stream, addr)) = await!(listener.accept()) {
        info!("connection from {}", addr);
        spawn(handle_stream(stream))?;
    }
    Ok(())
}

async fn handle_stream(mut stream: TcpStream) -> Result<(), Error> {
    await!(stream.write_str("Please enter filename: "))?;
    let file_name_vec = await!(stream.read())?;
    let file_name = String::from_utf8(file_name_vec)?.trim_matches(CRLF).to_owned();
    let file_contents = await!(read_to_string(file_name))?;
    await!(stream.write_str(&file_contents))?;
    stream.close();
    Ok(())
}
```

写这篇文章的主要目的是梳理和总结，同时也希望能给对这方面有兴趣的`Rustacean`作为参考。本文代码以易于理解为主要编码原则，某些地方并没有太考虑性能，还请见谅；但如果文章和代码中有明显错误，欢迎指正。

本文代码仓库在 [Github](https://github.com/Hexilee/async-io-demo) (部分代码较长，建议`clone`下来用编辑器看)，所有`examples`在`nightly-x86_64-apple-darwin 2018 Edition`上均能正常运行。运行`example/async-echo`时设置`RUST_LOG`为`info`可以在 terminal 看到基本的运行信息，`debug`则可见事件循环中的事件触发顺序。

## 异步 `IO` 的基石 - `mio`

`mio`是一个极简的底层异步`IO`库，如今`rust`生态中几乎所有的异步`IO`程序都基于它。

随着`channel`，`timer`等`sub module`在`0.6.5`版本被标为`deprecated`，如今的`mio`提供的唯二两个核心功能分别是：

- 对操作系统异步网络`IO`的封装
- 用户自定义事件队列

第一个核心功能对应到不同操作系统分别是：

- Linux(Android) => epoll
- Windows => iocp
- MacOS(iOS), FreeBSD => kqueue
- Fuchsia => <unknown>

mio 把这些不同平台上的 API 封装出了一套`epoll like`的异步网络 API，支持`udp`和`tcp`。

> *除此之外还封装了一些不同平台的拓展 API，比如`uds`，本文不对这些 API 做介绍。*

## 异步网络 I/O

下面是一个`tcp`的`demo`：

```rust
// examples/tcp.rs

use mio::*;
use mio::net::{TcpListener, TcpStream};
use std::io::{Read, Write, self};
use failure::Error;
use std::time::{Duration, Instant};

const SERVER_ACCEPT: Token = Token(0);
const SERVER: Token = Token(1);
const CLIENT: Token = Token(2);
const SERVER_HELLO: &[u8] = b"PING";
const CLIENT_HELLO: &[u8] = b"PONG";

fn main() -> Result<(), Error> {
    let addr = "127.0.0.1:13265".parse()?;

// Setup the server socket
    let server = TcpListener::bind(&addr)?;

// Create a poll instance
    let poll = Poll::new()?;

// Start listening for incoming connections
    poll.register(&server, SERVER_ACCEPT, Ready::readable(),
                  PollOpt::edge())?;

// Setup the client socket
    let mut client = TcpStream::connect(&addr)?;

    let mut server_handler = None;

// Register the client
    poll.register(&client, CLIENT, Ready::readable() | Ready::writable(),
                  PollOpt::edge())?;

// Create storage for events
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
                    let (handler, addr) = server.accept()?;
                    println!("accept from addr: {}", &addr);
                    poll.register(&handler, SERVER, Ready::readable() | Ready::writable(), PollOpt::edge())?;
                    server_handler = Some(handler);
                }

                SERVER => {
                    if event.readiness().is_writable() {
                        if let Some(ref mut handler) = &mut server_handler {
                            match handler.write(SERVER_HELLO) {
                                Ok(_) => {
                                    println!("server wrote");
                                }
                                Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => continue,
                                err => {
                                    err?;
                                }
                            }
                        }
                    }
                    if event.readiness().is_readable() {
                        let mut hello = [0; 4];
                        if let Some(ref mut handler) = &mut server_handler {
                            match handler.read_exact(&mut hello) {
                                Ok(_) => {
                                    assert_eq!(CLIENT_HELLO, &hello);
                                    println!("server received");
                                }
                                Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => continue,
                                err => {
                                    err?;
                                }
                            }
                        }
                    }
                }
                CLIENT => {
                    if event.readiness().is_writable() {
                        match client.write(CLIENT_HELLO) {
                            Ok(_) => {
                                println!("client wrote");
                            }
                            Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => continue,
                            err => {
                                err?;
                            }
                        }
                    }
                    if event.readiness().is_readable() {
                        let mut hello = [0; 4];
                        match client.read_exact(&mut hello) {
                            Ok(_) => {
                                assert_eq!(SERVER_HELLO, &hello);
                                println!("client received");
                            }
                            Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => continue,
                            err => {
                                err?;
                            }
                        }
                    }
                }
                _ => unreachable!(),
            }
        }
    };
    Ok(())
}
```

这个`demo`稍微有点长，接下来我们把它一步步分解。

直接看主循环

```rust
fn main() {
    // ...
    loop {
        poll.poll(&mut events, None).unwrap();
        // ...
    }
}
```

每次循环都得执行`poll.poll`，第一个参数是用来存`events`的`Events`， 容量是`1024`；

```rust
let mut events = Events::with_capacity(1024);
```

第二个参数是`timeout`，即一个`Option<Duration>`，超时会直接返回。返回类型是`io::Result<usize>`。

> *其中的`usize`代表`events`的数量，这个返回值是`deprecated`并且会在之后的版本移除，仅供参考*

这里我们设置了`timeout = None`，所以当这个函数返回时，必然是某些事件被触发了。让我们遍历`events`：

```rust
match event.token() {
      SERVER_ACCEPT => {
          let (handler, addr) = server.accept()?;
          println!("accept from addr: {}", &addr);
          poll.register(&handler, SERVER, Ready::readable() | Ready::writable(), PollOpt::edge())?;
          server_handler = Some(handler);
      }

      SERVER => {
          if event.readiness().is_writable() {
              if let Some(ref mut handler) = &mut server_handler {
                  match handler.write(SERVER_HELLO) {
                      Ok(_) => {
                          println!("server wrote");
                      }
                      Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => continue,
                      err => {
                          err?;
                      }
                  }
              }
          }
          if event.readiness().is_readable() {
              let mut hello = [0; 4];
              if let Some(ref mut handler) = &mut server_handler {
                  match handler.read_exact(&mut hello) {
                      Ok(_) => {
                          assert_eq!(CLIENT_HELLO, &hello);
                          println!("server received");
                      }
                      Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => continue,
                      err => {
                          err?;
                      }
                  }
              }
          }
      }
      CLIENT => {
          if event.readiness().is_writable() {
              match client.write(CLIENT_HELLO) {
                  Ok(_) => {
                      println!("client wrote");
                  }
                  Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => continue,
                  err => {
                      err?;
                  }
              }
          }
          if event.readiness().is_readable() {
              let mut hello = [0; 4];
              match client.read_exact(&mut hello) {
                  Ok(_) => {
                      assert_eq!(SERVER_HELLO, &hello);
                      println!("client received");
                  }
                  Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => continue,
                  err => {
                      err?;
                  }
              }
          }
      }
      _ => unreachable!(),
  }
```

我们匹配每一个`event`的`token`，这里的`token`就是我用来注册的那些`token`。比如我在上面注册了`server`：

```rust
// Start listening for incoming connections
poll.register(&server, SERVER_ACCEPT, Ready::readable(),
                  PollOpt::edge()).unwrap();
```

第二个参数就是`token`：

```rust
const SERVER_ACCEPT: Token = Token(0);
```

这样当`event.token() == SERVER_ACCEPT`时，就说明这个事件跟我们注册的`server`有关，于是我们试图`accept`一个新的连接并把它注册进 `poll`，使用的`token`是`SERVER`。