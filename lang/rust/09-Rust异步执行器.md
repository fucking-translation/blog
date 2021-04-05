# Rust 异步执行器

[原文](https://jblog.andbit.net/2019/11/10/rust-async-execution/)

作为从事大量并发工作(尤其是在 [Fanout](https://fanout.io/) 的网络工作)的老牌 C/C++ 程序员，[Rust](https://www.rust-lang.org/) 编程语言及其最近的[异步](https://blog.rust-lang.org/2019/11/07/Async-await-stable.html)功能引起了我的兴趣 (intrigued)。

像 C/C++ 一样，Rust 没有运行时。有趣的是，即使引入了异步也同样如此。使用`async`和`await`关键字可以并发的运行代码。你需要使用自己的运行时(如：Tokio 或 async-std) 与语言的核心元素进行交互。

但是，你也可以实现自己的运行时！在过去的几个月中，我一直在学习所有的实现细节。在本文中，我将描述如何仅使用标准库来执行 Rust 的异步函数。

关于 异步以及 future 其他文章已经写了很多，因此本文主要关注于如何构建执行程序。

## 语言 vs 运行时

Rust 提供了一下几种基本特性：

- [Future](https://doc.rust-lang.org/std/future/trait.Future.html) 特征：允许逐步执行某项操作。
- `async`关键字：重写你的代码以实现`Future`。
- `await`关键字：允许在生成的异步代码中使用其他的`Future`实例。

就是这样。值得注意的是，Rust 在你使用`async`关键字生成`Future`之外并没有提供`Future`的具体实现。

为了使用 Rust 的异步特性做一些有用的事情，你将需要一些`Future`实现(仅使用生成的`Future`是没有意义的)，以及一种执行`Future`实例的方法。

我个人认为这是一个出色的设计。Rust 能够提供相当不错的异步语法，而不用提交给特定的运行时。

请注意，尽管听起来好像语言本身没有提供太多的功能，但是其内置的异步代码生成却是一个具有[挑战性的问题](https://tmandry.gitlab.io/blog/posts/optimizing-await-1/)。

## 实现一个 Future

下面是对`Future`特征的定义：

```rust
pub trait Future {
    type Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output>;
}
```

你可以手动实现一个`Future`。举个例子：下面这个 Future 可以产生一个整数：

```rust
use std::future::Future;
use std::task::{Context, Poll};

struct IntFuture {
    x: i32,
}

impl Future for IntFuture {
    type Output = i32;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<i32> {
        Poll::Ready(self.x)
    }
}
```

或者你可以通过`async`关键字来构建具有同样功能的代码：

```rust
async fn int_future(x: i32) -> i32 {
    x
}
```

在以上两种情况下，我们最终都具有满足`Future<Output = i32>`的类型：

```rust
fn eat<F: Future<Output = i32>>(_: F) {}

fn main() {
    eat(IntFuture { x: 42 });
    eat(int_future(42));
}
```

## 嵌入的 future

如果你有一个异步函数的调用链，举个例子：一个 HTTP 请求的异步函数调用 TCP I/O 的异步函数，它将被编译为单个封装的`Future`。对这个 future 进行轮询将导致对其内部的 future 进行轮询。进行轮询的任何操作都不会对内部 future 有任何感知。

举个例子：

```rust
async fn get_audience() -> &'static str {
    "world"
}

async fn make_greeting() -> String {
    let audience = get_audience().await;

    format!("hello {}", audience)
}
```

在上面的代码中，如果调用`make_greeting()`来获取一个 future，轮询这个 future 将会依次轮询由`get_audience()`生成的 future，但这可以视为`make_greeting()`的实现细节。

所有这一切都说明：在执行 future 时，我们实际上只需要考虑最顶层的 future 即可。

## 调用 poll

创建一个`Future`有点简单。轮询 future，也没有太多内容。我们再来看一下`poll()`的签名：

```rust
fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output>;
```

我们需要一个`Pin`和一个`Context`。你可能要问，这些到底是什么？

## Pin

`Pin`是一种表示某些内存不会被移动的方式。通常，可以将结构从一个内存位置移动到另一个内存位置，而不会出现任何问题。这是因为 Rust 禁止在`safe`代码中进行自引用。举个例子，一个结构可以存储在栈上，然后被移动到堆上的`Box`中，并且 Rust 可以通过简单的复制字节来执行移动操作。然而，Rust 异步最伟大的成就之一就是可以在 await 点之间进行借用，这需要稍微调整规则。异步生成的 future 需要具有在使用 await 时保留对其内部内存引用的能力，因此需要确保其内存不会在`poll()`调用之间移动。

这里使用`Pin`的方式有点不直观。`poll()`函数消耗`Pin`。这意味着为了轮询一个 future，每次轮询的时候你都需要实例化一个新的`Pin`。看起来像这样：

```rust
let mut f = IntFuture { x: 42 };

let mut cx = ... // we'll talk about this later

let p = unsafe { Pin::new_unchecked(&mut f) };

match p.poll(&mut cx) {
    Poll::Ready(x) => println!("got int: {}", x),
    Poll::Pending => println!("future not ready"),
}
```

(请注意：有趣的是`Pin`可以被用作`self`类型。看起来 Rust 虽然将`self`限制为`T`，`&T`以及`&mut T`，还允许使用[固定列表](https://doc.rust-lang.org/reference/items/associated-items.html#methods)中的其他类型)。

一旦通过`poll()`方法消耗并消毁了`Pin`，你是否就不打算保留固定的内存了？没有！对[文档](https://doc.rust-lang.org/std/pin/struct.Pin.html#safety)的质疑，“这个值一旦固定，就必须永远固定”。事实上，这就是为什么构建`Pin`是 unsafe 的原因。unsafe 的部分是你最终会丢失`Pin`，但是尽管没有`Pin`保护你了，你仍然需要坚持固定 (pinning) 合约。

## Context 和 Waker

当前，`Context`唯一要做的就是提供对`Waker`的访问。`Waker`用于指示如果`poll()`返回了`Poll::Pending`，则应在何时再次轮询 future。`poll()`采用`Context`而不是简单的`Waker`是为了实现扩展。在更高版本的 Rust 中，其他内容可能会添加到`Context`上。

构建`Context`需要一些努力。它唯一 (sole) 的构造函数[Context::from_waker](https://doc.rust-lang.org/std/task/struct.Context.html#method.from_waker) 需要一个`Waker`。`Waker` 唯一的构造函数 [Waker::from_raw](https://doc.rust-lang.org/std/task/struct.Waker.html#method.from_raw) 需要一个`RawWaker`。并且`RawWaker`唯一的构造函数 [RawWaker::new](https://doc.rust-lang.org/std/task/struct.RawWakerVTable.html#method.new) 需要一个`RawWakerVTable`。

让我们实现一个迷你版，不带任何操作的`RawWakerVTable`：

```rust
use std::task::{RawWaker, RawWakerVTable};

unsafe fn vt_clone(data: *const ()) -> RawWaker {
    RawWaker::new(data, &VTABLE)
}

unsafe fn vt_wake(_data: *const ()) {
}

unsafe fn vt_wake_by_ref(_data: *const ()) {
}

unsafe fn vt_drop(_data: *const ()) {
}

static VTABLE: RawWakerVTable = RawWakerVTable::new(
    vt_clone,
    vt_wake,
    vt_wake_by_ref,
    vt_drop
);
```

然后我们可以像这样构建一个`Waker`：

```rust
let rw = RawWaker::new(&(), &VTABLE);

let w = unsafe { Waker::from_raw(rw) };
```

所有的这些 vtable 都是为了允许我们提供自己的唤醒行为。`RawWaker` 只是一个数据指针和一个 vtable。`Waker`对此进行了封装，并实现了熟悉的 Rust 特征，如`Clone`和`Drop`。`Waker`构造函数是 unsafe 的，因此 vtable 函数可能需要对原始指针进行解引用。

你可能想知道为什么 Rust 使用此自定义的 vtable 而不是使`Waker`成为特征。我相信这样做是为了使`Waker`可以被拥有，同时避免了堆分配。使用特征可能需要在某处添加`Box`。

最后，我们可以构建一个`Context`：

```rust
let mut cx = Context::from_waker(&w);
```

当然，在真实的应用中我们需要`Waker`做些什么。我们将在后面讨论这个问题。

## 这次是真的调用 poll() 了

既然我们知道如何构建一个`Pin`和一个`Context`，我们可以调用`poll()`了。以下是轮询一个 future 程序的完整源代码：

```rust
use std::future::Future;
use std::pin::Pin;
use std::task::{Poll, Context, Waker, RawWaker, RawWakerVTable};

unsafe fn vt_clone(data: *const ()) -> RawWaker {
    RawWaker::new(data, &VTABLE)
}

unsafe fn vt_wake(_data: *const ()) {
}

unsafe fn vt_wake_by_ref(_data: *const ()) {
}

unsafe fn vt_drop(_data: *const ()) {
}

static VTABLE: RawWakerVTable = RawWakerVTable::new(
    vt_clone,
    vt_wake,
    vt_wake_by_ref,
    vt_drop
);

async fn get_greeting() -> &'static str {
    "hello world"
}

fn main() {
    let mut f = get_greeting();

    let rw = RawWaker::new(&(), &VTABLE);
    let w = unsafe { Waker::from_raw(rw) };
    let mut cx = Context::from_waker(&w);

    let p = unsafe { Pin::new_unchecked(&mut f) };
    assert_eq!(p.poll(&mut cx), Poll::Ready("hello world"));
}
```

## 触发 Waker

让我们来创建一个知道如何唤醒自己的 future。

以下是计时器的实现。可以以期望的持续时间构建它。第一次轮询时，它会产生一个线程并返回`Poll::Pending`。下次轮询时，它将返回`Poll::Ready`。线程休眠然后调用`wake()`。

```rust
use std::time;
use std::thread;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

struct TimerFuture {
    duration: time::Duration,
    handle: Option<thread::JoinHandle<()>>,
}

impl TimerFuture {
    fn new(duration: time::Duration) -> Self {
        Self {
            duration,
            handle: None,
        }
    }
}

impl Future for TimerFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<()> {
        match &self.handle {
            None => {
                let duration = self.duration;
                let waker = cx.waker().clone();
                self.handle = Some(thread::spawn(move || {
                    thread::sleep(duration);
                    waker.wake();
                }));
                Poll::Pending
            },
            Some(_) => {
                let handle = self.handle.take().unwrap();
                handle.join().unwrap();
                Poll::Ready(())
            },
        }
    }
}

// convenience wrapper for use in async functions
fn sleep(duration: time::Duration) -> TimerFuture {
    TimerFuture::new(duration)
}
```

`Waker`已被克隆，因此我们可以在`poll()`返回后继续使用它。事实上，我们还可以将其移动到另一个线程。

请注意，在实际的应用程序中，你不希望为每个计时器都生成一个线程。取而代之的是，计时器可能会在某些事件反应堆 (evented reactor) 中注册。不过在此示例中，我们将使其保持简单。

## 管理不同类型的 future

在我们接触执行器之前，我们需要解决最后一个挑战：改变 future 的类型。

不同的 future 可以有不同的`Output`类型(如：`Future<Output = i32>`和`Future<Output = String>`)，因此，`poll()`也会有不同的返回值。这意味着如果我们要构建一个执行器，我们不能简单的通过将 future 放入类似`Vec<Box<dyn Future>>`的结构中，即使有可能，我们也无法使用相同的代码对其进行处理。

据我所知，解决方案是为执行器跟踪所有 future 选择一个共同的返回类型(即：顶级 future)。举个例子，你可以决定所有的顶级 future 都没有返回值类型，因此你可以将它们包含在`Vec<Box<dyn Future<Output = ()>>>`中。请注意，嵌套的 future 仍然可以具有任意的 (arbitrary) 返回值类型。一个不带返回值的异步函数可以等待一个返回`String`的 future。之所以可行是因为所有的嵌套 future 都隐藏在外部的 future 中，而执行者只关心外部的 future。

我们的类型问题还不止于此。`poll()`函数需要其具体类型的固定引用。回忆一下前面大写的`Self`签名。

```rust
fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output>;
```

这意味着即使两个不同的 future 实现都具有相同的`Output`类型并因此具有相同的特征，我们仍然无法使用非通用的代码来处理它们！

可能需要具体的类型，因此`Pin`可以保护一块已知大小的区域。无论如何，`dyn Future`都是没有用的。

解决此问题的一种方式是将细节隐藏在闭包中。我们可以使用单态化 (monomorphization) 为每个具体的 future 实现生成不同的代码，但是让闭包共享相同的函数签名。在下面，我们创建符合特征`dyn FnMut(&mut Context) -> Poll<()>`(对于带有`Output = ()`的 future) 的闭包，并将其装箱：

```rust
type PollFn = dyn FnMut(&mut Context) -> Poll<()>;

struct WrappedFuture {
    poll_fn: Box<PollFn>,
}

impl WrappedFuture {
    pub fn new<F>(mut f: F) -> Self
    where
        F: Future<Output = ()> + 'static
    {
        let c = move |cx: &mut Context| {
            let p: Pin<&mut F> = unsafe { Pin::new_unchecked(&mut f) };
            match p.poll(cx) {
                Poll::Ready(_) => Poll::Ready(()),
                Poll::Pending => Poll::Pending,
            }
        };

        Self {
            poll_fn: Box::new(c),
        }
    }

    pub fn poll(&mut self, cx: &mut Context) -> Poll<()> {
        (self.poll_fn)(cx)
    }
}
```

使用`WrappedFuture`，我们可以对所有 future 一视同仁：

```rust
// generates Future<Output = ()>
async fn print_hello() {
    println!("hello");
}

// generates Future<Output = ()>
async fn print_goodbye() {
    println!("goodbye");
}

fn main() {
    let mut futures: Vec<WrappedFuture> = Vec::new();

    futures.push(WrappedFuture::new(print_hello()));
    futures.push(WrappedFuture::new(print_goodbye()));

    for f in futures.iter_mut() {
        let mut cx = ... // context
        assert_eq!(f.poll(&mut cx), Poll::Ready(()));
    }
}
```

## 一个简单的执行器

为了执行我们的 future，我们需要做以下三件事：

- 在某处跟踪 future
- 当创建 future 时，对其进行轮询
- 实现`Waker`以便我们可以知道何时再次轮询 future

下面是一个基础的执行器。它使用两个向量(`need_poll`和`sleeping`)对 future 进行跟踪。调用`spawn`将一个 future 添加到`need_poll`中。

与其直接使用`WrappedFuture`，不如使用`Arc/Mutex`对其进行封装，以便可以在线程之间共享 future。我们声明一个别名(`SharedFuture`)，以减少噪音。

```rust
type SharedFuture = Arc<Mutex<WrappedFuture>>;

struct ExecutorData {
    need_poll: Vec<SharedFuture>,
    sleeping: Vec<SharedFuture>,
}

struct Executor {
    data: Arc<(Mutex<ExecutorData>, Condvar)>,
}

impl Executor {
    pub fn new() -> Self {
        let data = ExecutorData {
            need_poll: Vec::new(),
            sleeping: Vec::new(),
        };

        Self {
            data: Arc::new((Mutex::new(data), Condvar::new())),
        }
    }

    pub fn spawn<F>(&self, f: F)
    where
        F: Future<Output = ()> + 'static
    {
        let (lock, _) = &*self.data;

        let mut data = lock.lock().unwrap();

        data.need_poll.push(Arc::new(Mutex::new(WrappedFuture::new(f))));
    }

    pub fn wake(
        data: &mut Arc<(Mutex<ExecutorData>, Condvar)>,
        wf: &SharedFuture
    ) {
        let (lock, cond) = &**data;

        let mut data = lock.lock().unwrap();

        let mut pos = None;
        for (i, f) in data.sleeping.iter().enumerate() {
            if Arc::ptr_eq(f, wf) {
                pos = Some(i);
                break;
            }
        }
        if pos.is_none() {
            // unknown future
            return
        }

        let pos = pos.unwrap();

        let f = data.sleeping.remove(pos);
        data.need_poll.push(f);

        cond.notify_one();
    }

    pub fn exec(&self) {
        loop {
            let (lock, cond) = &*self.data;

            let mut data = lock.lock().unwrap();

            if data.need_poll.is_empty() {
                if data.sleeping.is_empty() {
                    // no tasks, we're done
                    break;
                }

                data = cond.wait(data).unwrap();
            }

            let need_poll = mem::replace(
                &mut data.need_poll,
                Vec::new()
            );

            mem::drop(data);

            let mut need_sleep = Vec::new();

            for f in need_poll {
                let w = MyWaker {
                    data: Arc::clone(&self.data),
                    f: Arc::new(Mutex::new(Some(Arc::clone(&f)))),
                }.into_task_waker();

                let mut cx = Context::from_waker(&w);

                let result = {
                    f.lock().unwrap().poll(&mut cx)
                };
                match result {
                    Poll::Ready(_) => {},
                    Poll::Pending => {
                        need_sleep.push(f);
                    },
                }
            }

            let mut data = lock.lock().unwrap();

            data.sleeping.append(&mut need_sleep);
        }
    }
}
```

`exec`函数循环并轮询 future。首先，它检查是否有 future 需要被轮询。如果没有，它将会等待一个休眠的 future 被唤醒。一旦有要轮询的 future，便对其进行轮询。如果轮询返回了`Ready`，表示 future 已经完成，我们可以将其释放。如果轮询返回了`Pending`，我们将 future 移动到`sleeping`向量中。如果没有剩余的 future，则循环退出。

为了唤醒一个执行器，需要调用`Executor::wake`。这是一个关联函数，旨在由`MyWaker`从另一个线程中调用。

`MyWaker`代码如下所示：

```rust
#[derive(Clone)]
struct MyWaker {
    data: Arc<(Mutex<ExecutorData>, Condvar)>,
    f: Arc<Mutex<Option<SharedFuture>>>,
}

impl MyWaker {
    ...

    fn wake(mut self) {
        self.wake_by_ref();
    }

    fn wake_by_ref(&mut self) {
        let f: &mut Option<SharedFuture> = &mut self.f.lock().unwrap();
        if f.is_some() {
            let f: SharedFuture = f.take().unwrap();
            Executor::wake(&mut self.data, &f);
        }
    }
}
```

唤醒器的实现旨在一次性使用，但是必须是可克隆的。这就是为什么内部`SharedFuture`由`Option`以及`Arc/Mutex`封装的原因。特定 future 的唤醒器集合可以安全共享单个`Option<SharedFuture>`的访问权限。在集合中的任何一个唤醒器上调用`wake()`时，都会唤醒 future，并将选项设置为`None`。

为了让我们的唤醒器实现可用，我们需要将其集成到 vtable 中，以便可以由`Waker`对其进行控制：

```rust
impl MyWaker {
    ...

    fn into_task_waker(self) -> Waker {
        let w = Box::new(self);
        let rw = RawWaker::new(Box::into_raw(w) as *mut (), &VTABLE);
        unsafe { Waker::from_raw(rw) }
    }

    ...
}

unsafe fn vt_clone(data: *const ()) -> RawWaker {
    let w = (data as *const MyWaker).as_ref().unwrap();
    let new_w = Box::new(w.clone());

    RawWaker::new(Box::into_raw(new_w) as *mut (), &VTABLE)
}

unsafe fn vt_wake(data: *const ()) {
    let w = Box::from_raw(data as *mut MyWaker);
    w.wake();
}

unsafe fn vt_wake_by_ref(data: *const ()) {
    let w = (data as *mut MyWaker).as_mut().unwrap();
    w.wake_by_ref();
}

unsafe fn vt_drop(data: *const ()) {
    Box::from_raw(data as *mut MyWaker);
}
```

基本上，以上 unsafe 代码将 vtable 函数连接到`MyWaker`的常规 Rust 方法，以处理克隆 (clone) 和销毁 (drop) 操作。

很明显，这不是最复杂的 (sophisticated) 执行器，但它足以作为示例。

## 使用执行器

让我们来试试看！`sleep`函数是我们之前定义的`TimerFuture`的封装。

```rust
fn main() {
    let e = Executor::new();

    e.spawn(async {
        println!("a");
        sleep(time::Duration::from_millis(200)).await;
        println!("c");
    });

    e.spawn(async {
        sleep(time::Duration::from_millis(100)).await;
        println!("b");
        sleep(time::Duration::from_millis(200)).await;
        println!("d");
    });

    e.exec();
}
```

以下输出如预期的一样：

```rust
a
b
c
d
```

完整的代码见[这里](https://github.com/jkarneges/rust-executor-example)。