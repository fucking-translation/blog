#  [「转」](https://weihanglo.tw/posts/2018/tokio-internals/)Tokio 內部機制：從頭理解 Rust 非同步 I/O 框架


<div style="text-align: center;">
  <img src="./img/welcome_to_the_futures.jpeg">
</div>

> 转载：[Tokio 內部機制：從頭理解 Rust 非同步 I/O 框架](https://weihanglo.tw/posts/2018/tokio-internals/)

[Tokio][tokio] 是 Rust 的開發框架，用於開發非同步 I/O 程式（asynchronous I/O，一種事件驅動的作法，可實現比傳統同步 I/O 更好的延伸性、效能與資源利用）。可惜的是，Tokio 過於精密的抽象設計，招致難以學習的惡名。即使我讀完教程後，依然不認為自己充分內化這些抽象層，以便推斷實際發生的事情。

從前的非同步 I/O 相關開發經驗甚至阻礙我學習 Tokio。我習慣使用作業系統提供的 selection 工具（例如 Linux epoll）當作起點，再轉移至 dispatch、state machine 等等。倘若直接從 Tokio 抽象層出發，卻沒有清楚了解 `epoll_wait()` 在何處及如何發生，我會覺得難以連結每個概念。Tokio 與 future-driven 的方法就好像一個黑盒子。

我決定不繼續由上而下的方法學習 Tokio，反其道而行，而是透過閱讀原始碼，確切理解具體實作是如何驅動從 epoll 事件到 `Future::poll()` 消耗 I/O 的整個過程。我不會深入高層次的 Tokio 與 futures 使用細節，[現有的教程][tokio-start] 有更完整詳細的內容。除了簡短的小結，我也不會探討一般性的非同步 I/O 問題，畢竟這些問題都可寫個獨立的主題了。我的目標是有信心讓 futures 與 Tokio 以我所認知的方式執行。

首先，有些重要的聲明。請注意，Tokio 正快速開發中，這裡所見所聞可能不久就會過時。這個研究中我用了 `tokio-core 0.1.10`、`futures- 0.1.17` 與 `mio 0.6.10`。由於我想從最底層理解 Tokio，我並不會考慮更高層次的套件如 `tokio-proto` 與 `tokio-service`。tokio-core 的事件系統本身有許多細節，為了精簡，我會盡量避開這些細項。我在 Linux 作業系統上研究 Tokio，而有些討論細節與作業系統相依，如 epoll。最後，這裡所有東西都是我這個 Tokio 新手的詮釋，可能會有錯誤或誤導。

[tokio-internals]: https://cafbit.com/post/tokio_internals/
[david-simmons]: https://www.davidsimmons.com/
[tokio]: https://tokio.rs/
[tokio-start]: https://tokio.rs/docs/getting-started/tokio/

## Asynchronous I/O in a nutshell

同步 I/O 程式會執行阻塞性的 I/O 操作，直到操作完成。例如讀取會阻塞至資料抵達，寫入會阻塞線程直到欲傳遞的 bytes 送達 kernel。這些操作非常適合依序執行的傳統命令式程式設計。舉例來說，一個 HTTP 伺服器替每個新連線產生一個新線程，這個線程會讀取資料並阻塞線程直到接收完整的 request，之後處理請求，再來阻塞線程至資料完全寫入 response。這是個方法非常直觀，缺點是會阻塞線程，因此每個連線的線程要各自獨立，每個線程也需有自己的 stack。然而，線程開銷阻礙了伺服器處理大量連線的可延伸性（參閱 [C10k problem][c10k])，對低階系統來說也不易負荷。

如果 HTTP server 使用非同步 I/O 開發，換句話說，在同一個線程上處理所有 I/O 操作。如此一來，所有活躍的連線以及 socket 監聽都會配置為非阻塞狀態（non-blocking），並在 event loop 中監控讀取與寫入是否就緒，進而在事件發生時分派給對應的處理程式（handler）。而每個連線都需維護自身的狀態與 buffer，如果一個處理程式一次僅能從 200 bytes 的 request 中讀取 100 個位元組（bytes），它就不能等待剩下的 bytes 而造成線程阻塞，處理程式必須將部分資料儲存在 buffer 中，設定當前的狀態為「讀取請求中」，並返回給 event loop。待到下一次連線調用的相同的處理程式，它才可讀取剩餘的 bytes 並將狀態轉為「寫入回應中」。如此的資源管理系統將會非常迅速，但同時也產生更複雜的 state machine 與容易出錯的毛病。

理想中的非同步 I/O 框架應該要提供能寫出近似於同步 I/O 的程式，但底層是 event loop 與 state machine。這對每個語言來說都很不容易，不過 Tokio 的實現已接近了。

[c10k]: https://wikipedia.org/wiki/C10k_problem

## The Tokio stack

![](https://cafbit.com/resource/tokio/tokio-stack.svg)

Tokio 的技術棧由下列幾個部分組成：

1. **The system selector**。每個作業系統皆提供接收 I/O 事件的工具，如 epoll（linux）、`kqueue()`（FreeBSD/macOS），與 IOCP（Windows）。
2. **Mio - Metal I/O**。[Mio][mio] 是一個 Rust crate，提供低階通用的 I/O API，內部處理特定作業系統的 selector 實作細節，所以你不需再處理這件事。
3. **Futures**。[Futures][futures] 以強大的抽象來表示尚未發生的事物。這些 future 以許多好用的方式組合成另一新的複合 future 來代表一系列複雜的事件。這個抽象層足以通用於許多 I/O 之外的事件，但在 Tokio 中
，我們專注在利用 futures 開發非同步 I/O state machines。
4. **Tokio**。[tokio-core][tokio-core] 提供一個中心的 event loop，這個 event loop 整合 Mio 回應 I/O 事件，並驅動 futures 完成（completion）。
5. **Your program**。一個採用 Tokio 框架的程式，會以 futures 操作非同步 I/O，並將這些 futures 傳遞給 Tokio 的 event loop 來執行。

[mio]: https://docs.rs/mio/0.6.10/mio/
[futures]: https://docs.rs/futures/0.1.17/futures/
[tokio-core]: https://docs.rs/tokio-core/0.1.10/tokio_core/

## Mio: Metal I/O

Mio 旨在提供一系列低階的 I/O API，允許調用端接收事件，如 socket 讀寫就緒狀態（readiness state）改變等。重點如下：

1. **Poll 與 Evented**。Mio 提供 [`Evented`][mio-evented] trait 來表示任何可當作事件來源的事物。在你的 event loop 中，你會利用 [`mio::Poll`][mio-poll] 物件註冊一定數量的 `Evented`，再調用 [`mio::Poll::poll`][mio-poll-poll] 來阻塞 loop，直到一至多個 `Evented` 產生事件（或超時）。

2. **System selector**。Mio 提供可跨平台的 system selector 訪問，所以 Linux epoll、Windows IOCP、FreeBSD/macOS `kqueue()`，甚至許多有潛力的平台都可調用相同的 API。不同平台使用 Mio API 的開銷不盡相同。由於 Mio 是提供基於 readiness（就緒狀態）的 API，與 Linux epoll 相似，不少 API 在 Linux 上都可以一對一映射。（例如：`mio::Events` 實質上是一個 `struct epoll_event` 陣列。）對比之下，Windows IOCP 是基於完成（completion-based）而非基於 readiness 的 API，所以兩者間會需要較多橋接。Mio 同時提供自身版本的 `std::net` struct 如 `TcpListener`、`TcpStream` 與 `UdpSocket`。這些 API 封裝 `std::net` 版本的 API，預設為非阻塞且提供 `Evented` 實作讓其將 socket 加入 system selector。

3. **Non-system events**。Mio 除了提供從 I/O 所得的 readiness 狀態來源，也可以用來指示從 user-space 來的 readiness 事件（非系統事件）。舉例來說，當一個工作線程（worker thread）完成一單位的工作，它就可以向 event loop 發出完成信號。你的程式調用 [`Registration::new2()`][mio-registration-new2] 以取得一個 `(Registration, SetReadiness)` 元組。`Registration` 是一個實作 `Evented` 且藉由 Mio 註冊在 event loop 的物件；而需要指示當前 readiness 狀態時，則會調用 [`SetReadiness::set_readiness`][mio-set_readiness]。在 Linux 上，非系統事件通知以 pipe 實作，當調用 `SetReadiness::set_readiness()` 時，會將 `0x01` 這個位元組寫入 pipe 中。而 `mio::Poll` 底層的 epoll 會配置為監控 pipe 讀取結束，所以 `epoll_wait()` 會解除阻塞，而 Mio 就可以將事件傳遞到調用端。另外，無論註冊多少非系統事件，都只會在 Poll 實例化時建立唯一一個 pipe。

每個 `Evented` 的註冊皆與一個由調用端提供 `usize` 型別的 [`mio::Token`][mio-token] 綁定，這個 token 將會與事件一起返回，以指示出對應的註冊資訊。這種作法很好地映射到 Linux 的 system selector，因為 token 可以放置在 64-bit 的 `epoll_data` union 中，並保持相同的功能。

這裡提供一個 Mio 操作的實際案例，下面是我們在 Linux 上使用 Mio 監控一個 UDP socket 的情況：

1. **建立 socket**。
    ```rust
    let socket = mio::net::UdpSocket::bind(
        &SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::new(127,0,0,1)),
            2000
        )
    ).unwrap();
    ```
    建立一個 Linux UDP socket，其中封裝一個 `std::net::UdpSocket`，再封裝在 `mio::net::UdpSocket` 中。這個 socket 為非阻塞性（non-blocking）。

2. **建立 poll 實例**。
    ```rust
    let poll = mio::Poll::new().unwrap();
    ```
    在這步驟，Mio 初始化 system selector、readiness 佇列（用於非系統事件），以及併發保護。當 readiness 佇列初始化時，會建立一個 pipe，讓 readiness 從 user-space 發出信號，而這個 pipe 的檔案描述符（file descriptor）會加入 epoll 中。每個 `Poll` 物件建立時，都會賦予一個獨特、遞增的 `selector_id`。

3. **透過 poll 註冊 socket**。
    ```rust
    poll.register(
        &socket,
        mio::Token(0),
        mio::Ready::readable(),
        mio::PollOpt::level()
    ).unwrap();
    ```
    `UdpSocket` 的 `Evented::register()` 被調用時，會將代理指向一個封裝的 `EventedFd`，這個 `EventedFd` 會將 socket 的 file descriptor 加入 poll selector 中（最終會調用 `epoll_ctl(fepd, EPOLL_CTL_ADD, fd, &epoll_event)`，而 `epoll_event.data` 設置為傳入的 token 值）。當一個 `UdpSocket` 註冊後，`selector_id` 會設置到與傳入的 `Poll` 相同，從而與 selector 產生連結。

4. **在 event loop 中呼叫 `poll()`**。
    ```rust
    loop {
        poll.poll(&mut events, None).unwrap();
        for event in &events {
            handle_event(event);
        }
    }
    ```
    System selector（`epoll_wait()`）與 readiness 佇列將會輪詢（poll）新的事件。（`epoll_wait()` 會阻塞，但由於非系統事件是透過 pipe 出發 epoll，事件仍會即時處理。）這一系列組合的事件可供調用端處理。

[mio-evented]: https://docs.rs/mio/0.6.10/mio/event/trait.Evented.html
[mio-poll]: https://docs.rs/mio/0.6.10/mio/struct.Poll.html
[mio-poll-poll]: https://docs.rs/mio/0.6.10/mio/struct.Poll.html#method.poll

[mio-registration-new2]: https://docs.rs/mio/0.6.10/mio/struct.Registration.html#method.new2
[mio-set_readiness]: https://docs.rs/mio/0.6.10/mio/struct.SetReadiness.html#method.set_readiness 
[mio-token]: https://docs.rs/mio/0.6.10/mio/struct.Token.html

## Futures and Tasks

[Futures][wiki-futures] 是從函數式程式設計借來的技術，一個尚未完成的運算會以一個 future 代表，而這些獨立的 future 可以組合起來，開發更複雜的系統。這個概念對非同步 I/O 非常中用，因為在處理交易（transaction）的所有基礎步驟，都可以模化為合成 futures（combinded futures）。以 HTTP 伺服器為例，一個 future 讀取 request，會從接收到有效資料開始讀取到 request 結束，另一個 future 則會處理這個 request 並產生 response，再另一個 future 則會寫入 responses。

在 Rust 中，[futures crate][futures] 實現了 futures。你可以透過實作 [Future][futures-future] trait 來定義自己的 future，這個 trait 需實現 [`poll()`][futures-poll] 方法，這個方法會在需要時調用，允許 future 開始執行。`poll()` 方法會回傳一個錯誤（error），或回傳一個指示告知 future 仍在處理，或是當 future 完成時返回一個值。`Future` trait 也提供許多組合操作子（combinator）作為預設方法。

欲理解 futures，須先探討三個重要的概念：**tasks**、**executors**，以及 **notifications**，且需理解此三者該如何安排，才能在正確時間點調用 future 的 `poll()` 方法。每一個 future 都在一個 task 語彙環境（context）中執行。一個 task 只與一個 future 關聯，而這個 future 卻可能是一個合成的 future，驅動其他封裝的 futures。（舉例來說，多個 future 用 `join_all()` 組合操作子，串連成單一一個 future，或是兩個 future 利用 `and_then()` 組合操作子來依序執行。）

Task 與它的 futures 需要被一個 _executor_ 執行。一個 executor 的責任是在正確時間點輪詢 task/future，輪詢通常會在接收到執行進度開始的通知時。而這個通知將在一個實作 [`futures::executor::Notify`][futures-notify] trait 的物件調用 [`notify`][futures-notify] 時發布。這裡有個例子，是由 futures crate 所提供的非常簡單的 executor，在調用 future 上的 [`wait()`][futures-wait] 被呼叫。擷自[原始碼][futures-task-source]：

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

給定一個融合 task 與 future 的 [`futures::executor::Spawn`][futures-executor-spawn] 物件，這個 executor 在迴圈中調用 [`poll_future_notify`][poll-future-notify]。這個 `Notify` 會成為 task 執行語彙環境的一部分，future 也會被輪詢。如果一個 future `poll` 方法回傳 `Async::NotReady`，表示 future 仍等待中，必須在往後再次輪詢。`Notify` object 會從 [`futures::task::current()`][futures-task-current] 取得一個指向 task 的 handle，且在 future 有些進展時調用 [`notify()`][futures-task-notify] 方法。（當一個 future 被輪詢時，與該 future 相關的 task 訊息將會儲存到 thread-local 中，thread-local 可以透過 `current()` 存取取得。）上例中，如果輪詢回傳 `Async::NotReady`，executor 會阻塞至接收到通知。也許 future 在其他線程運算，在完成時調用 `notify()`；或是 `poll()` 方法在返回 `Asynx::NotReady` 之前，自身直接調用了 `notify()`（後者並不常見，因為理論上一個 `poll()` 在返回之前應該持續取得進展）。 

Tokio 的 event loop 行為上比簡單整合「 Mio 事件驅動 future 完成」來得精細。舉例來說，一個 Mio event 表示一個 socket 的 readiness（就緒狀態），最後會產生一個通知，足以告知相對應的 future 需要輪詢。

處理 future 時，Task 是最基礎的執行單元，且基本上就是[綠色線程][wiki-green-threads]，提供[協調式多工][wiki-cooperative-multitasking]，允許在同一個系統線程有多個執行語彙環境。當一個 task 無法有所進展，會讓處理器先處理其他可執行的 task。我們必須理解的是，「通知」會發生在 task 層級而非 future 層級。當一個 task 被通知時，它會輪詢它連結的最高層級的 future，這會導致任何或是全部的 child future 同樣被輪詢。例如，如果一個 task 最高層級的 future 是一個以 [`join_all`][futures-join-all] 組合的十個 future，而其中一個 future 安排要通知此一 task，則無論需不需要，全部十個 future 皆須接受輪詢。

[wiki-futures]: https://wikipedia.org/wiki/Futures_and_promises
[futures-Future]: https://docs.rs/futures/0.1.17/futures/future/trait.Future.html
[futures-poll]: https://docs.rs/futures/0.1.17/futures/future/trait.Future.html#tymethod.poll
[futures-notify]: https://docs.rs/futures/0.1.17/futures/executor/trait.Notify.html
[futures-notify-notify]: https://docs.rs/futures/0.1.17/futures/executor/trait.Notify.html#tymethod.notify
[futures-wait]: https://docs.rs/futures/0.1.17/futures/future/trait.Future.html#method.wait
[futures-task-source]: https://github.com/alexcrichton/futures-rs/blob/0.1.17/src/task_impl/std/mod.rs#L233
[futures-executor-spawn]: https://docs.rs/futures/0.1.17/futures/executor/struct.Spawn.html
[poll-future-notify]: https://docs.rs/futures/0.1.17/futures/executor/struct.Spawn.html#method.poll_future_notify
[futures-task-current]: https://docs.rs/futures/0.1.17/futures/task/fn.current.html
[futures-task-notify]: https://docs.rs/futures/0.1.17/futures/task/struct.Task.html#method.notify
[wiki-green-threads]: https://en.wikipedia.org/wiki/Green_threads
[wiki-cooperative-multitasking]: https://en.wikipedia.org/wiki/Cooperative_multitasking
[futures-join-all]: https://docs.rs/futures/0.1.17/futures/future/fn.join_all.html

## Tokio's interface with Mio

Tokio 利用上述的 Mio 「非系統事件」，將 task 通知轉換為 Mio 的事件。在取得一個 Mio 的 (`Registration`、`SetReadiness`）元組後，Tokio 會將 `Registration`（一個 `Evented`）註冊至 Mio 的 poll （event loop）中，再將 `SetReadiness` 封裝在實作了 `Notify` trait 的 `MySetReadiness` 中。[原始碼][tokio-mysetreadiness]如下：

```rust
struct MySetReadiness(mio::SetReadiness);

impl Notify for MySetReadiness {
    fn notify(&self, _id: usize) {
        self.0.set_readiness(mio::Ready::readable())
              .expect("failed to set readiness");
    }
}
```

在這個作法中，task 的通知將轉換為 Mio 事件，且可以透過 Tokio 的事件處理與分派機制與其他 Mio 事件作伙處理。

如同 Mio 封裝 `std::net` 內的 `UdpSocket`、`TcpListener`，以及 `TcpStream` 來客製化需求，Tokio 也利用了組合（composition）與裝飾（decoration）建立這些型別的 Tokio 版。舉例來說，Tokio 的 `UdpSocket` 架構大致如下：

![](https://cafbit.com/resource/tokio/udpsocket.svg)

Tokio 版本的 I/O 來源型別的建構子都需要傳入 event loop 的 handle（[`tokio_core::reactor::Handle`][tokio-handle]）。當實例化時，這些型別會將它們的 socket 註冊至 Mio poll 的 event loop 上，以利接收 edge-triggred（譯注：一種 epoll event 的觸發模式）的事件及其新賦予的偶數數字 token（以下會解釋）。當底層的 I/O 操作回傳 `WouldBlock` 時，這些型別可以很方便地安排當前的 task 來接收讀寫的 readiness。

Tokio 在 Mio 上註冊了許多 `Evented` 型別，儲存在特定的 token 上：

- **Token 0（`TOKEN_MESSAGES`）**：用於 Tokio 內部的消息佇列（message queue），這個佇列提供移除 I/O 來源、接收讀寫 readiness 通知的 task 排程，設定 timeout，以及執行在 event loop 語彙環境中的任意閉包。這個 token 可以安全地從其他線程與 event loop 溝通。例如，[`Remote::spawn()`][tokio-spawn] 透過訊息系統，將 future 送達 event loop。

    實作上，消息佇列是一個 [`futures::sync::mpsc`][futures-mpsc] stream。身為一個 [`futures::stream::Stream`][futures-stream]（與 future 類似，但是產生一序列的值而非單一值），消息佇列使用上述 `MySetReadiness` 方案來處理，而 `Registration` 則是以 `TOKEN_MESSAGES` 這個 token 註冊。當接收到 `TOKEN_MESSAGES` 事件時，該事件會分派到 `consume_queue()` 方法進一步處理。（原始碼：[`enum Message`][tokio-reactor-enum-message]、[`consume_queue()`][tokio-reactor-consume-queue]）
- **Token 1（`TOKEN_FUTURE`）**：用於通知 Tokio 需要輪詢 main task。這個 token 會在與 main task 相關聯的通知上（也就是傳入 `Core::run()` 的 future 或它的子 future，而非透過 `spawn()` 在不同 task 中執行的 future）。這個事件同樣用了 `MySetReadiness` 方案將 future 轉譯成 Mio 的事件。在一個 future 被 main task 執行前，會先回傳 `Async::NotReady`，並以其所選的方式在稍後發布通知。當接收了 `TOKEN_FUTURE` 事件，Tokio event loop 就會再次輪詢 main task。

- **大於 1 的偶數 token（`TOKEN_START + key * 2`）**：用來指示 I/O 來源的 readiness 改變。Token 中的 key 是 `Slab` key，關聯值是 `Core::inner::io_dispatch Slab<ScheduledIo>`。當 Mio 的 I/O 來源型別（`UdpSocket`、`TcpListener`、`TcpStream`）實例化之初，會自動以此 token 註冊。

- **大於 1 的奇數 token（`TOKEN_START + key * 2 + 1`）**：用來指示一個 spawned task（及其關聯的 future）需要被輪詢。Token 中的 key 是 `Slab` key，關聯值是 `Core::inner::task_dispatch Slab<ScheduledTask>`。和 `TOKEN_MESSAGES` 與 `TOKEN_FUTURE` 事件相同，這個事件也用了 `MySetReadiness` 溝通。

[tokio-mysetreadiness]: https://github.com/tokio-rs/tokio-core/blob/0.1.10/src/reactor/mod.rs#L791
[tokio-handle]: https://docs.rs/tokio-core/0.1.10/tokio_core/reactor/struct.Handle.html
[tokio-spawn]: https://docs.rs/tokio-core/0.1.10/tokio_core/reactor/struct.Remote.html#method.spawn
[futures-mpsc]: https://docs.rs/futures/0.1.17/futures/sync/mpsc/index.html
[futures-stream]: https://docs.rs/futures/0.1.17/futures/stream/trait.Stream.html
[tokio-reactor-enum-message]: https://github.com/tokio-rs/tokio-core/blob/0.1.10/src/reactor/mod.rs#L133
[tokio-reactor-consume-queue]: https://github.com/tokio-rs/tokio-core/blob/0.1.10/src/reactor/mod.rs#L403

## Tokio event loop

Tokio，更精確來說是 [`tokio_core::reactor::Core`][tokio-reactor-core] 提供了 event loop 來管理 futures 和 tasks，驅動 future 完成，以及與 Mio 介接的介面，讓 I/O 事件可正確通知對應的 task。使用 event loop 需透過 [`Core::new()`][tokio-reactor-core-new] 實例化一個 `Core`，並調用 [`Core::run()`][tokio-reactor-core-run] 傳入一個 future。這個 event loop 在返回之前，將會驅動傳入的 future 至完成。以伺服器程式來說（serve application），這個 future 很可能生命週期較長，例如使用 `TcpListener` 持續接收新傳入的連結，每個連結透過 [`Handle.spawn()`][tokio-reactor-handle-spawn] 分別建立 task，由自身的 future 獨立處理。

以下的流程圖大略點出 Tokio event loop 的基本輪廓：

![](https://cafbit.com/resource/tokio/tokio-event-loop.svg)

[tokio-reactor-core]: https://docs.rs/tokio-core/0.1.10/tokio_core/reactor/struct.Core.html
[tokio-reactor-core-new]: https://docs.rs/tokio-core/0.1.10/tokio_core/reactor/struct.Core.html#method.new
[tokio-reactor-core-run]: https://docs.rs/tokio-core/0.1.10/tokio_core/reactor/struct.Core.html#method.run
[tokio-reactor-handle-spawn]: https://docs.rs/tokio-core/0.1.10/tokio_core/reactor/struct.Handle.html#method.spawn

## What happens when data arrives on a socket?

想了解 Tokio，可以觀察當資料抵達 socket 時，event loop 發生的每個步驟。我很訝異地發現，這個過程最終分為兩部分，分別在 event loop 內各自的迭代中，進行各自的 epoll 交易處理。第一部分負責當 socket 讀取就緒時（例如，Mio 事件帶著比 1 大的偶數 token，或 main task 的 `TOKEN_FUTURE`），傳送通知到對該 socket 有興趣的 task；第二部分則是透過輪詢 task 與它的 future 來處理通知（例如，Mio 事件帶著比 1 大的奇數 token）。我們來了解以下情境：一個 spawned task 從 Linux 上的 `UdpSocket`，透過 Tokio event loop 讀取資料，並假設前一次輪詢結果導致 `recv_from()` 回傳一個 `WouldBlock` 錯誤。

![](https://cafbit.com/resource/tokio/recv-sequence-1.svg)

Tokio event loop 調用 `mio::Poll:poll()`，該方法轉而調用 `epoll_wait()`（在 Linux 上）進而阻塞到某個監測中的 file descriptor 發生了 readiness 改變的事件。當上述情形發生後，`epoll_wait()` 回傳一個 `epoll_event` structs 的陣列，用以描述發生什麼事，這些 structs 也將透過 Mio 轉譯為 `mio::Events`，並返回 Tokio。（在 Linux 上，這些轉譯應該是零成本（zero-cost），因為 `mio::Events` 就只是簡單，以一個 `epoll_event` 陣列組成的元組結構（tuple struct）。）在我們的例子，假設在陣列中只有一個事件指出 socket 已讀取就緒。由於該事件的 token 是大於 1 的偶數，Tokio 辨識其為 I/O 事件，並從 `Slab<ScheduledIo>` 中尋找對應的元素，以取得有哪些 task 對這個 socket 的讀寫 readiness 狀態有興趣。接下來，Tokio 會通知對讀取有興趣的 task，這些 task 透過前述的 `MySetReadiness`，調用 Mio 的 `set_readiness()`。Mio 會將這個非系統的事件詳細資訊加到 readiness 佇列中，並寫入 `0x01` 到 readiness pipe 中。

![](https://cafbit.com/resource/tokio/recv-sequence-2.svg)

在 Tokio event loop 往下一個迭代前進之前，它會再次輪詢 Mio，Mio 則調用 `epoll_wait()`，而 `epoll_wait()` 這次返回一個在 Mio 的 readiness pipe 上發生的讀取 readiness 事件。Mio 讀取之前寫入的 `0x01`，並從 readiness 佇列取出最前端（dequeue）的非系統事件資料，並將這個事件回傳到 Tokio。由於該事件的 token 是大於 1 的奇數 token，Tokio 辨識其為 task 通知事件，並從 `Slab<ScheduledTask>` 中尋找對應的元素，以取得 task 從 `spawn()` 回傳的最原始的 `Spawn` 物件。接下來，Tokio 透過 [`poll_future_notify()`][futures-spawn-poll-future-notify] 輪詢這個 task 與它的 future，這個 future 可能會從 socket 讀取資料，直至得到 `WouldBlock` 錯誤。

這個二迭代的方法涉及了 pipe 讀寫，對比其他非同步 I/O event loop，可能會有一點額外開銷。如果在一個單線程的程式中，使用 `strace` 會看到一個線程用 pipe 與自己溝通，很是奇怪：

```c
pipe2([4, 5], O_NONBLOCK|O_CLOEXEC) = 0
...
epoll_wait(3, [{EPOLLIN|EPOLLOUT, {u32=14, u64=14}}], 1024, -1) = 1
write(5, "\1", 1) = 1
epoll_wait(3, [{EPOLLIN, {u32=4294967295, u64=18446744073709551615}}], 1024, 0) = 1
read(4, "\1", 128) = 1
read(4, 0x7ffce1140f58, 128) = -1 EAGAIN (Resource temporarily unavailable)
recvfrom(12, "hello\n", 1024, 0, {sa_family=AF_INET, sin_port=htons(43106), sin_addr=inet_addr("127.0.0.1")}, [16]) = 6
recvfrom(12, 0x7f576621c800, 1024, 0, 0x7ffce1140070, 0x7ffce114011c) = -1 EAGAIN (Resource temporarily unavailable)
epoll_wait(3, [], 1024, 0) = 0
epoll_wait(3, 0x7f5765b24000, 1024, -1) = -1 EINTR (Interrupted system call)
```

Mio 選用 pipe 的方案來支持通用性，以防 `set_readiness()` 可能被其他線程調用。也有可能這種作法對強制實施公平的事件調節與維持 futures 與 I/O 的間接層有所幫助。

[futures-spawn-poll-future-notify]: https://docs.rs/futures/0.1.17/futures/executor/struct.Spawn.html#method.poll_future_notify

## Lessons learned: Combining futures vs. spawning futures

最初探索 Tokio 時，我寫了一個小程式，負責監聽不同 UDP socket 進來的資料。這個程式建立十個讀取 socket 的 future 實例，每個都監聽不同的埠口（port）。我天真地使用 [`join_all()`][futures-join-all] 將所有 future 合成為單一 future，並將之傳入 `Core::run()`，訝異的是，我發現每當一個封包送達，所有 future 都會輪詢一次。另一個驚訝的點是，`tokio_core::net:UdpSocket::recv_from()`（以及底層的 [`PollEvented`][tokio-pull-evented]）非常聰明，當 socket 在前一次的 Mio 輪詢中尚未標記為讀取就緒時，會避免調用作業系統 `rectfrom()`。以下的 `strace` 反映出我寫的 future `poll()` 的除錯 `println!()`，大致如下：

```c
epoll_wait(3, [{EPOLLIN|EPOLLOUT, {u32=14, u64=14}}], 1024, -1) = 1
write(5, "\1", 1) = 1
epoll_wait(3, [{EPOLLIN, {u32=4294967295, u64=18446744073709551615}}], 1024, 0) = 1
read(4, "\1", 128) = 1
read(4, 0x7ffc183129d8, 128) = -1 EAGAIN (Resource temporarily unavailable)
write(1, "UdpServer::poll()...\n", 21) = 21
write(1, "UdpServer::poll()...\n", 21) = 21
write(1, "UdpServer::poll()...\n", 21) = 21
write(1, "UdpServer::poll()...\n", 21) = 21
write(1, "UdpServer::poll()...\n", 21) = 21
write(1, "UdpServer::poll()...\n", 21) = 21
write(1, "UdpServer::poll()...\n", 21) = 21
recvfrom(12, "hello\n", 1024, 0, {sa_family=AF_INET, sin_port=htons(43106), sin_addr=inet_addr("127.0.0.1")}, [16]) = 6
getsockname(12, {sa_family=AF_INET, sin_port=htons(2006), sin_addr=inet_addr("127.0.0.1")}, [16]) = 0
write(1, "recv 6 bytes from 127.0.0.1:43106 at 127.0.0.1:2006\n", 52) = 52
recvfrom(12, 0x7f2a11c1c400, 1024, 0, 0x7ffc18312ba0, 0x7ffc18312c4c) = -1 EAGAIN (Resource temporarily unavailable)
write(1, "UdpServer::poll()...\n", 21) = 21
write(1, "UdpServer::poll()...\n", 21) = 21
write(1, "UdpServer::poll()...\n", 21) = 21
epoll_wait(3, [], 1024, 0) = 0
epoll_wait(3, 0x7f2a11c36000, 1024, -1) = ...
```

有鑑於 Tokio 與 futures 的具體內部運作某個程度上對我來說有點隱晦，我想我希望背後有些魔法路由，可以只輪詢必要的 futures。當然，對 Tokio 有更深入的理解後，我的程式很明顯這樣利用 futures：

![](https://cafbit.com/resource/tokio/futures-join.svg)

這的確可以執行，但不夠好，尤其是當你有一拖拉庫 socket 時。由於通知在 task 層級發生，上圖中任意一個綠色方格中通知都會導致 main task 被通知。它將會輪詢 `FromAll` future 使得所有 `FromAll` 的 child future 都須接受輪詢。我真正需要的是一個簡單的 main future，使用 `Handle::spawn()` 來啟動每個封裝在各自的 task 中的 future。這種安排大致如下圖：

![](https://cafbit.com/resource/tokio/futures-spawn.svg)

當任何 future 安排一個通知，只有該 future 的 task 會收到通知，也只有該 future 會被輪詢（回想一下，「安排一個通知」會自動發生在 `tokio_core::net:UdpSocket::rect_from()` 從 `mio::net::UdpSocket::rect_from()` 回傳值中接收到 `WouldBlock` ）。future 組合操作子敘述表達能力強勁，可好整以暇地描述協議（protocol）的流程而不須弄髒手寫手動輪詢的狀態機。然而重要的是，你必須理解你的設計也許需要支援各自獨立，獨自且並行運作的 tasks。（譯注：而非都在 main task 上使用 `join_all()`）

[tokio-pull-evented]: https://docs.rs/tokio-core/0.1.10/tokio_core/reactor/struct.PollEvented.html

## Final thoughts

閱讀 Tokio、Mio 以及 futures 原始碼後，大大幫助我鞏固對 Tokio 的理解，也驗證了透過理解具體實作來釐清抽象層的學習策略。這個方法在僅僅學習抽象層的狹隘使用案例時非常危險，我們必須意識到具體的示例僅是助於理解一般通例。在閱讀完原始碼之後，再次閱讀 Tokio 的教學文件，我有些馬後炮的意見：**Tokio 非常合理，應該要很容易理解與上手！**

我仍有些問題待日後研究：

- Tokio 有處理 edge triggering（Linux `epoll`）的飢餓問題（starvation problem）嗎？我認為這個問題可以在 future 中，以單一一個 `poll()` 限制讀 / 寫的數量。當達到這個限制時，future 可以在顯式通知當前 task 提前返回，而非依靠 Tokio I/O 來源類型的隱式「`WouldBlock` 排程」行為。因此這使得其他 task 與 future 有機會有所進展。
- Tokio 是否不依賴於將工作卸載給工作線程（worker thread）以最大化處理器核心運用，而是直接支援多線程環境下執行 event loop 嗎？

> **2017-12-19 更新**：這裡有 Reddit 對話串討論本文。Mio 的作者 Carl Lerche 在[這裡][reddit-comment-1]和[這裡][reddit-comment-2]貼了些資訊量充足的留言。除了回應上述問題，他也點出 [`FuturesUnordered`][futures-stream-futures-unordered] 是一種合成 futures 的方法，只有相關的 child future 會被輪詢，以避免所有 future 像使用 `join_all()` 全部輪詢，不過這方法有些額外的記憶體配置開銷要衡量。另外，未來的 Tokio 將要遷離 `mio::Registration` 的通知方案，目的是簡化前述一些步驟。
> <!-- -->
>    
> <!-- -->
> **2017-12-21 更新**：看起來 Hacker News 也有在[討論這篇文章][hn-discussion]。

[reddit-discussion]: https://redd.it/7klghl
[reddit-comment-1]: https://www.reddit.com/r/rust/comments/7klghl/tokio_internals_understanding_rusts_asynchronous/drfw5n1/
[reddit-comment-2]: https://www.reddit.com/r/rust/comments/7klghl/tokio_internals_understanding_rusts_asynchronous/drfwc1m/
[futures-stream-futures-unordered]: https://docs.rs/futures/0.1.17/futures/stream/struct.FuturesUnordered.html
[hn-discussion]: https://news.ycombinator.com/item?id=15972593