# 使用 Mio 编写底层 TCP 服务器

[原文](https://sergey-melnychuk.github.io/2019/08/01/rust-mio-tcp-server/)

是时候认识 (acquainted) 一下 [Metal IO](https://github.com/tokio-rs/mio) 了，它是在 epoll/kqueue 之上用 Rust 编写的跨平台抽象。

在本文中，我们将会展示并解释如何编写一个单线程异步 TCP 服务器，用它模拟 HTTP 协议，然后使用`ab/wrk`对其进行 benchmark。结果将会令人印象深刻。

## Getting started

我使用的是`mio = "0.6"`。

首先，需要 TCP listener。

```rust
let address = "0.0.0.0:8080";
let listener = TcpListener::bind(&address.parse().unwrap()).unwrap();
```

然后创建`Poll`对象并将 listener 注册到`Token(0)`中用于可读事件 (readable events)，由 edge (而不是 level) 激活。更多内容请参阅 [edge vs level](https://en.wikipedia.org/wiki/Epoll#Triggering_modes)。

```rust
let poll = Poll::new().unwrap();
poll.register(
    &listener, 
    Token(0),
    Ready::readable(),
    PollOpt::edge()).unwrap();
```

下一步我们要做的就是根据给定的容量创建`Events`对象以及主循环(本例中是无限循环)。在循环中，事件被一一轮询并处理。

```rust
let mut events = Events::with_capacity(1024);
loop {
    poll.poll(&mut events, None).unwrap();
    for event in &events {
        // handle the event
    }
}
```

## Accepting connections (and dropping them)

事件可以是以下其中一种：

- listener 上的可读事件意味着有要准备接入的连接。
- 已连接的 socket 上的事件
 - readable - socket 有数据可以读取
 - writable - socket 已经写数据就绪

listener 以及 socket 事件可以被 token 区分，对于 listener token 它总是 0，因为它已在`Poll`中注册。

以下代码是最简单的事件处理方式，在循环中接受所有的传入连接，并且对于每个连接 - 只需删除 socket。它将会关闭连接。在你的服务中[抛弃协议](https://en.wikipedia.org/wiki/Discard_Protocol)。

```rust
// handle the event
match event.token() {
    Token(0) => {
        loop {
            match listener.accept() {
                Ok((socket, address)) => {
                    // What to do with the connection?
                    // One option is to simply drop it!
                    println!("Got connection from {}", address);
                },
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock =>
                    // No more connections ready to be accepted 
                    break,
                Err(e) => 
                    panic!("Unexpected error: {}", e)
            }
        }
    },
    _ => () // Ignore all other tokens
}
```

listener 的`.accept()`方法返回`std::io::Result<(TcpStream, SocketAddr)>`(见 [accept](https://docs.rs/mio/0.5.1/mio/tcp/struct.TcpListener.html#method.accept))，因此我需要匹配并处理成功的响应或者错误。这里有一个特定的错误类型 [io::ErrorKind::WouldBlock](https://doc.rust-lang.org/nightly/std/io/enum.ErrorKind.html#variant.WouldBlock)，它表示“我将等待(阻塞)以取得任何进展”。这是非阻塞 (non-blocking) 行为的本质 - 关键是不要阻塞(而是返回相应的错误)！遇到此类错误时，意味着此时没有更多的传入连接等待接入，因此循环中断，并处理下一个事件。

现在如果我运行服务器并尝试和它建立连接，我可以看到正在抛弃协议！是不是很神奇？

```console
$ nc 127.0.0.1 8080
$ 
```

## Registering connections for events

接着说下一个事件。为了发生下一个事件，首先必须使用`Poll`注册 token-socket 对。在底层 (under the hook)，`Poll`将会跟踪哪一个 token 对应哪一个 socket，但是客户端代码只能访问 token。这意味着如果服务器打算与客户端进行实际通信(我很确信大多数服务器都这样做)，就必须以某种方式存储 token-socket 对。在本例中，我使用了简单的`HashMap<Token, TcpStream>`，但是使用 [slab](https://docs.rs/slab/0.4.2/slab/) 可能会更加高效。

token 只是`usize`的一个封装器，因此简单的计数器就足以提供递增的 token 序列。一旦使用相应的 token 注册了 socket，它就会被插入到`HashMap`中。

```rust
let mut counter: usize = 0;
let mut sockets: HashMap<Token, TcpStream> = HashMap::new();

// handle the event
match event.token() {
    Token(0) => {
        loop {
            match listener.accept() {
                Ok((socket, _)) => {
                    counter += 1;
                    let token = Token(counter);

                    // Register for readable events
                    poll.register(&socket, token
                        Ready::readable(),
                        PollOpt::edge()).unwrap();

                    sockets.insert(token, socket);                    
                },
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock =>
                    // No more connections ready to be accepted 
                    break,
                Err(e) => 
                    panic!("Unexpected error: {}", e)
            }
        }
    },
    token if event.readiness().is_readable() => {
        // Socket associated with token is ready for reading data from it
    }
}
```

## Reading data from client

当给定 token 发生可读事件时，意味着数据在相应的 socket 中读就绪。我将只使用字节数组作为读取数据的缓冲区。

在循环中执行读取操作，直到返回已知的`WouldBlock`错误。每次调用`read`将返回(如果成功的话)实际读取的字节数，当读取的字节数为 0 时 - [意味着](https://doc.rust-lang.org/nightly/std/io/trait.Read.html#tymethod.read)客户端已经断开连接，此后保持 socket (或继续循环读取)没有意义。

```rust
// Fixed size buffer for reading/writing to/from sockets
let mut buffer = [0 as u8; 1024];
...
token if event.readiness().is_readable() => {
    loop {
        let read = sockets.get_mut(token).unwrap().read(&mut buffer);
        match read {
            Ok(0) => {
                // Successful read of zero bytes means connection is closed
                sockets.remove(token);
                break;
            },
            Ok(len) => {
                // Now do something with &buffer[0..len]
                println!("Read {} bytes for token {}", len, token.0);
            },
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) => panic!("Unexpected error: {}", e)
        }
    }
}
...
```

## Writing data to the client

对于接收可写事件的 token，它必须先在`Poll`中注册。`oneshot`选项对于安排可写事件可能很有用，该选项确保感兴趣的 (interest) 事件只被触发一次。

```rust
poll.register(&socket, token
    Ready::writable(),
    PollOpt::edge() | PollOpt::oneshot()).unwrap();
```

向客户端 socket 写入数据与之类似，也是通过缓冲区完成的，但是不需要显式循环，因为已经有一个[方法](https://doc.rust-lang.org/std/io/trait.Write.html#method.write_all)在执行循环：`write_all()`。

如果我想让协议返回接收到的字节数，我将需要写入的实际字节数(`HashMap`将会做这件事)，在发生可读事件时计算字节数，然后安排一次可写事件，以及何时发生可写事件 - 然后发送响应并断开连接。

```rust
let mut response: HashMap<Token, usize> = HashMap::new();
...
token if event.readiness().is_readable() => {
    let mut bytes_read: usize = 0;
    loop {
        ... // sum up number of bytes received
    }
    response.insert(token, bytes_read);
    // re-register for one-shot writable event
}
...
token if event.readiness().is_writable() => {
    let n_bytes = response[&token];
    let message = format!("Received {} bytes\n", n_bytes);
    sockets.get_mut(&token).unwrap().write_all(message.as_bytes()).unwrap();
    response.remove(&token);
    sockets.remove(&token); // Drop the connection
},
```

## What happens between reading and writing data?

此时我已经从 socket 上读取了数据，并且将数据写入 socket 中。但是写入事件永远也不会发生，因为没有为可写事件注册 token！

我应该什么时候为可写事件注册 token？好吧，当它有东西要写入的时候(进行注册)！听起来很简单，不是吗？在实践中，这意味着要真正实现一些协议了。

## How do I implement a protocol?

我只想发回文本(或 JSON)，而 [TCP](https://ru.wikipedia.org/wiki/Transmission_Control_Protocol) 是一种[协议](https://en.wikipedia.org/wiki/Communication_protocol)，一种传输级的传输控制协议。TCP 关心接收方以发送方发送的确切顺序来接收确切数量的字节！所以在传输级别，我必须处理两个字节流：一个从客户端到服务端，另一个直接返回。

与服务器打交道时应用层协议会很有用(如 HTTP)。应用层协议可以定义实体，如`request` - 服务器从客户端接收，以及`response` - 客户端从服务器接收回来。

值得一提的是，正确实现 HTTP 并不像听起来那么容易。但是已经有现成的 HTTP 库可供使用(如 [hyper](https://github.com/hyperium/hyper))。在这里，我不会为如何实现 HTTP 而烦恼，我要做的是让我的服务器表现的好像它真的理解 GET 请求，但总会用包含 6 个字节的响应来应答这样的请求：`b"hello \n"`。

## Mocking HTTP

对于本文而言，mock HTTP 已经绰绰有余。我将把 HTTP 请求头与请求体(如果有的话)用 4 个字节`b"\r\n\r\n"`进行分割。因此，如果我跟踪当前客户端发送的内容，并且在任何时候那里都有 4 个字节，我就可以使用预定义的 HTTP 响应进行应答：

```plain
HTTP/1.1 200 OK
Content-Type: text/html
Connection: keep-alive
Content-Length: 6

hello
```

`HashMap`就已经足够用于跟踪所有接收到的字节。

```rust
let mut requests: HashMap<Token, Vec<u8>> = HashMap::new();
```

一旦读取结束，就需要检查请求是否已就绪：

```rust
fn is_double_crnl(window: &[u8]) -> bool {  /* trivial */ }

let ready = requests.get(&token).unwrap()
    .windows(4)
    .find(|window| is_double_crnl(*window))
    .is_some();
```

如果已就绪，则可以安排一些数据写入！

```rust
if ready {
    let socket = sockets.get(&token).unwrap();
    poll.reregister(
        socket,
        token,
        Ready::writable(),
        PollOpt::edge() | PollOpt::oneshot()).unwrap();
}
```

写入完成之后，重要的是要保持连接打开，并重新注册 socket 以再次读取。

```rust
poll.reregister(
    sockets.get(&token).unwrap(),
    token,
    Ready::readable(),
    PollOpt::edge()).unwrap();
```

服务器已就绪！

```console
$ curl localhost:8080
hello
```

好戏开始了 - 让我们看看这个单线程服务器表现如何。我将会使用常用的工具：`ab`和`wrk`。

- `ab`需要使用`-k`选项以使用`keep-alive`并重用已有连接。
- `wrk2`实际与`wrk`用法相同，因此需要`--rate`参数。
- `ab/wrk`运行在不同的 VM 上而不是在服务器上(但是在相同的 region 中)。

以下是我在某个云提供商的实例`n1-standard-8 (8 vCPUs, 30 GB memory)`上尝试对服务器进行 benchmark 时得到的数字：

```console
$ ab -n 1000000 -c 128 -k http://instance-1:8080/
<snip>
Requests per second:    105838.76 [#/sec] (mean)
Transfer rate:          9095.52 [Kbytes/sec] received
```

```console
$ wrk -d 60s -t 8 -c 128 --rate 150k http://instance-1:8080/
<snip>
Requests/sec: 120596.75
Transfer/sec: 10.12MB
```

对于单线程来说，105k 与 120k 的 rps 不算太差。

当然，这次可以当作是作弊，但只要涉及真实网络(即使在同一区域内)，这就是负载下的真实服务器，这可能(或多或少)是使用单线程完成此网络速度的重要底线。

完成可运行的代码地址是：[github](https://github.com/sergey-melnychuk/mio-tcp-server)，每一个 pull-request 由一个逻辑章节组成：

- 初始化项目：[PR#1](https://github.com/sergey-melnychuk/mio-tcp-server/pull/1)
- accept & discard: [PR#2](https://github.com/sergey-melnychuk/mio-tcp-server/pull/2)
- read from socket：[PR#3](https://github.com/sergey-melnychuk/mio-tcp-server/pull/3)
- writing to socket：[PR#4](https://github.com/sergey-melnychuk/mio-tcp-server/pull/4)
- mocking HTTP：[PR#5](https://github.com/sergey-melnychuk/mio-tcp-server/pull/5)

## Where to go from here

扩展到多线程：从[这里](https://blog.cloudflare.com/the-sad-state-of-linux-socket-balancing/)开始。