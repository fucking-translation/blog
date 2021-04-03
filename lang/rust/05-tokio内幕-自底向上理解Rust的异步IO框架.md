# tokio 内幕：自底向上的理解 Rust 的异步 I/O 框架

[原文](https://cafbit.com/post/tokio_internals/)

</br>

[tokio](https://tokio.rs/) 是用于开发异步 I/O 应用程序的 Rust 框架 - 它基于事件驱动的方式，与传统的同步 I/O 相比，它通常可以实现更好的可伸缩性，性能与资源使用。不幸的是，tokio 由于其复杂的 (sophisticated) 抽象而难以学习。即使在阅读了教程之后，我仍然觉得自己没有内部化抽象来推理出实际发生的事情。

我以前在异步 I/O 编程方面的经验甚至可能成为我学习 tokio 的阻碍。我习惯于使用操作系统的 selection 工具(如 Linux 的 epoll) 作为起点，然后继续学习调度，状态机等内容。从 tokio 的抽象开始，对底层的 epoll_wait 发生的位置和方式没有清晰的了解，我发现很难将所有的点连接起来。tokio 及其 future 驱动的方式感觉就像个黑匣子。

我没有继续采用自上而下的方式来学习 tokio，而是决定采用自下而上的方式，通过研究源代码来确切地了解当前在`Future::poll()`中的具体实现如何驱动 epoll 事件向 I/O 消耗的进展。我不会详细介绍 tokio 和 futures 的高级用法，因为有[其他的教程](https://tokio.rs/docs/getting-started/tokio/)对其进行了更深入的说明。除了做一些简短的总结之外，我也不会讨论异步 I/O 的一般问题，因为讨论该主题可能需要写一本书。我的目标仅仅是让人们对 futures 和 tokio 轮询的工作方式充满信息。

首先，先做一些重要的免责声明。请注意，tokio 正在积极的开发中，因此此处的某些结论可能很快就会过时。为了这次研究的目的，我使用`tokio-core: 0.1.10`，`futures: 0.1.17`以及`mio: 0.6.10`。因为我想了解 tokio 的最底层，所以我没有考虑使用诸如`tokio-proto`和`tokio-service`这种更高层次的 crate。tokio-core 的事件系统拥有很多的组件，为了尽可能简介，本文将不会对其进行讨论。我是在 Linux 系统中学习 tokio 的，因此一些讨论必然涉及平台相关的实现细节，如：epoll。最后，这里提到的所有内容都是我作为 tokio 新手对其做出的解释，因此可能存在错误或误解。

## 异步 I/O 简述

同步 I/O 编程涉及执行 同步的 I/O 操作，该操作会一直阻塞线程直到完成。读操作会一直阻塞直到接收到数据，写操作会一直阻塞直到输出的字节发送到内核为止。这种依次执行的操作十分适合传统的命令式编程方式。举个例子，一个 HTTP 服务器为每一个连接都创建了一个线程。在线程中，它需要一直读取字节流直到接收到整个请求(在接收到所有的字节之前线程值阻塞的)，然后处理请求，再将响应写入连接通道(在所有的字节写入之前，线程是阻塞的)。这是一种非常直接的方式。缺点(downside)是由于阻塞，每个连接都需要一个不同的线程，每个线程都有自己的堆栈。在多数情况下这都没有问题，同步 I/O 是一种正确的方式。然而，线程开销阻碍了服务器处理大量连接的可伸缩性(参阅：[C10k问题](https://en.wikipedia.org/wiki/C10k_problem))，并且在处理少量连接的低配置系统中也可能过载。

如果我们的 HTTP 服务器是使用异步 I/O 编写的，它可能会在一个线程中处理所有的 I/O 操作。所有的活跃连接以及监听套接字都将被配置成非阻塞的，在事件循环 (event loop) 中监听其读/写的准备情况，并在事件发生时将执行操作分发给处理程序。每个连接需要维护其状态和缓冲区。如果一个处理程序只能读取 200 字节请求的其中100个字节，它将无法等待剩余字节的到达，因为这样做会阻止其他准备处理的连接。它必须将部分读取存储在缓冲区中，将状态设置为“读请求”，然后返回事件循环。下次此连接调用处理程序时，它可以读取请求的剩余部分并切换为“写入响应”的状态。借助复杂的状态机和易于出错的资源管理，实现这样的系统非常麻烦。

一个理想的异步 I/O 框架将提供一种依次编写此类 I/O 处理步骤的方式，就好像它们在阻塞一样，但是在后台会生成事件循环和状态机。在大多数语言中这都是一个艰巨的任务，但是 Tokio 让我们可以更接近这种方式。

## Tokio 技术栈

![tokio-stack](./img/tokio-stack.svg)

Tokio 技术栈包含以下几个组件：

1. **System Selector**： 每个操作系统都提供了一个接收 I/O 事件的工具，如：epoll (Linux)，kqueue (FreeBSD/Mac OS)以及 IOCP (Windows)。

2. **Mio - Metal I/O**：[Mio](https://docs.rs/mio/0.6.10/mio/) 是一个 Rust 库，它通过内部处理每个操作系统的细节来提供一套用于底层 I/O 的通用 API。

3. **Futures**：[Futures](https://docs.rs/futures/0.1.17/futures/) 为尚未发生的事件提供了一个强大的抽象。它用一种有效的方式将这些事件组合在一起，以创建可以描述复杂事件序列的组合 Futures。这种抽象足够通用，可以用于 I/O 之外的许多其他的方面，在 Tokio 中，我们将异步 I/O 的状态机作为 futures。

4. **Tokio**：[tokio-core](https://docs.rs/tokio-core/0.1.10/tokio_core/) 提供了中央事件循环，该循环与 Mio 集成以响应 I/O 事件，并驱动完成 futures。

5. **你的程序**：使用 Tokio 框架的程序可以将异步 I/O 系统构造为 futures，并为其提供 Tokio 事件循环以待执行。

## Mio：Metal I/O

Mio 提供了一套底层 I/O 的 API，它允许调用者接收诸如读写套接字准备情况变更的事件，以下为其中的重点：

1. **Poll 和 Evented**：Mio 提供了一个 [Evented](https://docs.rs/mio/0.6.10/mio/event/trait.Evented.html) 特征来表示任何可能成为事件源的东西。在你的事件循环中，你可以通过 [mio::Poll](https://docs.rs/mio/0.6.10/mio/struct.Poll.html) 对象来注册大量的 `Evented`，然后调用 [mio::Poll::poll()](https://docs.rs/mio/0.6.10/mio/struct.Poll.html#method.poll) 进行阻塞直到事件发生在一个或多个 `Evented` 对象中(或超过了指定的超时时间)。

2. **System Selector**：Mio 对 System selector 提供了跨平台的访问方式，因此 Linux 的 epoll，Windows 的 IOCP，FreeBSD/Mac OS 的 kqueue 以及其他选择都可以使用相同的 API。system selector 适应 Mio API 的开销有所不同。因为 Mio 提供了一种类似于 epoll 基于就绪的 API，因此在 Linux 中使用 Mio时，API 的许多部分可以一对一的进行映射(例如：`mio::Events`本质上是一个结构为`epoll_event`的数组)。相反，由于 Windows 的 IOCP 是基于完成而不是基于就绪，因此需要更多的适配来桥接这两个范式。Mio 提供了自己的`std::net`的结构，如：`TcpListener`，`TcpStream`以及`UdpSocket`。它们封装了`std::net`，但是默认为非阻塞形式，并提供了`Evented`实现，这些实现将套接字添加到 System selector 中。

3. 非系统事件：除了提供了 I/O 源的就绪状态之外，Mio 还可以指示在用户空间中生成就绪事件。举个例子，如果工作线程完成了一个工作单元，则它可以向事件循环线程发出完成信号。你的程序调用 [Registration::new2()](https://docs.rs/mio/0.6.10/mio/struct.Registration.html#method.new2) 以获取一个(`Registration`, `SetReadiness`) 元组。`Registration` 对象是一个`Evented`，可以在事件循环中向 Mio 注册。当需要指示就绪情况时，可以在`SetReadiness`对象上调用[set_readiness()](https://docs.rs/mio/0.6.10/mio/struct.SetReadiness.html#method.set_readiness)。在 Linux 中，非系统事件通知是使用管道实现的。当调用`SetReadiness::set_readiness()`时，`0x01`字节就被写入管道中。`mio::Poll`的基础 epoll 配置为监控管道读取的末端，因此`epoll_wait()`将解除阻塞并且 Mio 可以将事件传递给调用方。实例化轮询时仅创建一个管道，而不管后来注册了多少个(如果有)非系统事件。

每一个`Evented`的注册都与调用者提供的作为`mio::Token`的`usize`类型的值相关联，并且此值与事件一起返回以指示相应的注册。在 Linux 系统中，这可以很好的映射到 System selector 中，因为 token 可以放置在 64 位 `epoll_data` 联合体中，该联合体以相同的方式起作用。

为了提供 Mio 操作的具体示例，这是当我们使用 Mio 监视 Linux 系统上的 UDP 套接字时在内部发生的事情：

1. **创建套接字**

```rust
let socket = mio::net::UdpSocket::bind(
    &SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(127,0,0,1)),
        2000
    )
).unwrap();
```

这里创建了一个 Linux 中封装在`std::net::UdpSocket`的 UDP 套接字，这个套接字也封装在`mio::net::UdpSocket`中。这个套接字被设置为非阻塞的。

2. **创建 poll**

```rust
let poll = mio::Poll::new().unwrap();
```

Mio 初始化 System selector，就绪队列(用于非系统事件)和并发保护。就绪队列初始化会创建一个管道，以便可以从用户空间发出准备就绪的信号，并将管道读取的文件描述符添加到epoll中。创建 `poll`对象时，将从递增计数器中为其分配唯一的`selector_id`。

3. **使用 poll 注册套接字**

```rust
poll.register(
    &socket,
    mio::Token(0),
    mio::Ready::readable(),
    mio::PollOpt::level()
).unwrap();
```

`UdpSocket`的`Evented.register()`函数被调用时，会将代理指向一个被封装的`EventedFd`，这个`EventedFd`会将套接字的文件描述符添加到 poll selector 中(最终会调用`epoll_ctl(fepd, EPOLL_CTL_ADD, fd, &epoll_event)`，并将`epoll_event.data`设置为提供的token值)。当一个`UdpSocket`被注册后，它的`selector_id`会被设置为`Poll`的`selector_id`，从而与 selector 产生关联。

4. **在事件循环中调用 poll()**

```rust
loop {
    poll.poll(&mut events, None).unwrap();
    for event in &events {
        handle_event(event);
    }
}
```

system selector (`epoll_wait()`)和就绪队列将会轮询是否有新事件(`epoll_wait()会阻塞，但是由于非系统事件除了推送到就绪队列之外，还通过管道触发了epoll，因此仍需要及时处理它们。`)。这一系列事件的组合可供调用端处理。

## Futures 和 任务

[Futures](https://en.wikipedia.org/wiki/Futures_and_promises) 是从函数式编程中借用的技术，因此尚未发生的计算可以表示为一个 “future“，并且这些独立的 future 可以被组合起来以构建一个复杂的系统。这对于异步 I/O 很有用，因为执行事物的基本步骤可以建模成此类组合的 futures。在 HTTP 服务器的示例中，一个 future 可以通过读取字节来读取一个请求，直到到达请求的末端为止，此时将产生请求对象。另一个 future 可能会处理请求并产生响应。再另一个 future 可能会写入响应。

在 Rust 中，[futures 库](https://docs.rs/futures/0.1.17/futures/) 实现了 futures，你可以通过实现 [Future](https://docs.rs/futures/0.1.17/futures/future/trait.Future.html) 特征来定义一个 future，它需要实现一个[poll()](https://docs.rs/futures/0.1.17/futures/future/trait.Future.html#tymethod.poll) 方法，该方法在需要时会被调用，并允许 future 开始执行。此方法会返回一个错误或表示 future 仍在等待中，因此应稍后再调用`poll()`，或者当 future 已经完成时将产生一个值。`Future` 特征还提供了大量的组合器作为默认方法。

想要理解 futures，先要理解三个重要的概念：任务，执行器，通知 - 以及它们是如何在正确的时间调用 futures 的`poll()`方法的。每一个 future 都会在一个 [任务](https://docs.rs/futures/0.1.17/futures/task/index.html)上下文中执行。一个任务直接与一个 future 相关联，但是这个 future 可能是个组合 future，它驱动着很多被包含的 future(举个例子，许多 future 通过[join_all()](https://docs.rs/futures/0.1.17/futures/future/fn.join_all.html) 组合器组合到一个future 中，或者两个 future 通过 [and_then()](https://docs.rs/futures/0.1.17/futures/future/trait.Future.html#method.and_then) 组合器依次执行)。

任务和他们的 future 需要一个执行器来运行。一个执行器需要在正确的时间轮询任务/ future - 通常是当获得通知可以做一些进展时。当其他一些代码调用实现了 [futures::executor::Notify](https://docs.rs/futures/0.1.17/futures/executor/trait.Notify.html) 特征所提供 [notify()](https://docs.rs/futures/0.1.17/futures/executor/trait.Notify.html#tymethod.notify) 方法的对象时，就会产生这样的通知。futures 库中提供的一个及其简单的执行程序就是一个例子，当在 future 上调用 [wait()](https://docs.rs/futures/0.1.17/futures/future/trait.Future.html#method.wait) 方法时，该执行程序将被调用。查看[源代码](https://github.com/alexcrichton/futures-rs/blob/0.1.17/src/task_impl/std/mod.rs#L233))：

```rust
/// Waits for the internal future to complete, blocking this thread's
/// execution until it does.
///
/// This function will call `poll_future` in a loop, waiting for the future
/// to complete. When a future cannot make progress it will use
/// `thread::park` to block the current thread.
pub fn wait_future(&mut self) -> Result<F::Item, F::Error> {
    ThreadNotify::with_current(|notify| {

        loop {
            match self.poll_future_notify(notify, 0)? {
                Async::NotReady => notify.park(),
                Async::Ready(e) => return Ok(e),
            }
        }
    })
}
```

给定一个预先创建的 [futures::executor::Spawn](https://docs.rs/futures/0.1.17/futures/executor/struct.Spawn.html) 对象来融合任务与 future，这个执行器在循环中调用 [poll_future_notify()](https://docs.rs/futures/0.1.17/futures/executor/struct.Spawn.html#method.poll_future_notify)。`Notify`对象变成任务上下文的一部分，future 也在被轮询。如果一个 future 的`poll()`返回`Async::NotReady`表明 future 仍在等待中，需要在 future 中再次安排轮询。`Notify`对象可以通过 [futures::task::current()](https://docs.rs/futures/0.1.17/futures/task/fn.current.html) 获取一个任务的句柄，并在 future 有进展后调用 [notify()](https://docs.rs/futures/0.1.17/futures/task/struct.Task.html#method.notify) 方法(当一个 future 正在被轮询时，与其关联的任务信息被存储在 thread-local 中，可以通过`current()`访问到)。在上述示例中，如果轮询返回了`Async::NotReady`，执行器将会一直阻塞直到接收到通知。也许 future 会在另一个线程中开始一些工作，并在完成时调用`notify()`，也许`poll()`在返回`Async::NotReady`之前直接自己调用`notify()`(后者不是很常见，因为从理论上来说，`poll()`应该在返回之前继续取得进展)。

Tokio 事件循环更像是一种复杂的 (sophisticated) 执行器，与Mio 事件集成以驱动 future 完成。在这种情况下，指示套接字准备就绪的 Mio 事件将发送一个通知使得对应的 future 进行轮询。

处理 future 时，任务是最基础的执行单元，且基本上就是提供了某种多任务协作的[绿色线程](https://en.wikipedia.org/wiki/Green_threads)，允许一个操作系统线程中有多个执行上下文。如果一个任务无法取得进展，会让处理器处理其他可执行的任务。理解通知发生在任务级别而非 future 级别是十分中重要的。当一个任务接收到通知，它将会轮询它的顶级 future，可能会导致其中某些或全部的子 future 都被轮询。举个例子，如果一个任务的顶级 future 是由其他 10 个 future `join_all()`的，其中一个 future 安排的任务被通知到，全部的 10 个任务都将被轮询。

## Tokio 与 Mio 的接口

Tokio 通过上面描述的 Mio “非系统事件”的特性将任务通知转换成为 Mio 的事件。当任务获取到 Mio 的`(Registration, SetReadiness)` 元组后，它使用 Mio 的轮询将`Registration`(它是一个`Evented`)进行注册，并在`MySetReadiness`中封装实现了`Notify`特征的`mio::SetReadiness`对象，查看[源码](https://github.com/tokio-rs/tokio-core/blob/0.1.10/src/reactor/mod.rs#L791)：

```rust
struct MySetReadiness(mio::SetReadiness);

impl Notify for MySetReadiness {
    fn notify(&self, _id: usize) {
        self.0.set_readiness(mio::Ready::readable())
              .expect("failed to set readiness");
    }
}
```

在这种方式中，任务通知被转换成为 Mio 事件，且可以在 Tokio 事件处理与分派机制中与其他类型的 Mio 事件一起进行处理。

就像 Mio 封装了`std::net`结构(如：`UdpSocket`，`TcpListener`，`TcpStream`)以自定义功能一样，Tokio 也使用组合和装饰来提供这些类型的 Tokio的 感知版本。例如：Tokio 的`UdpSocket`看起来像这样：

![udpsocket](./img/udpsocket.svg)