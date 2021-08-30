# 为什么 Rust 需要 Pin 和 Unpin

[原文](https://blog.adamchalmers.com/pin-unpin/)

使用 Rust 异步框架通常很简单。只需要编写正常的 Rust 代码，并添加`async`或`.await`即可。但是如果要编写你自己的异步框架可能会比较困难。当我第一次尝试时，被神秘 (arcane) 又深奥 (esoteric) 的语法弄的十分迷惑(如`T: ?Unpin`和`Pin<&mut Self>`)。我在之前从没见过这些类型，我也不知道它们起了什么作用。在本文中，我们将重点介绍关于这方面的内容，我们将会讨论：

- 什么是 Future
- 什么是自引用 (self-referential) 类型
- 为什么它们是 unsafe 的
- `Pin/Unpin`是如何让它们变成 safe 的
- 使用`Pin/Unpin`编写棘手的嵌套 future

## 什么是 Future？

几年前，我需要编写一些代码以收集某些函数运行指标(如执行时间)。我想要编写一个如下所示的`TimedWrapper`类型：

```rust
// Some async function, e.g. polling a URL with [https://docs.rs/reqwest]
// Remember, Rust functions do nothing until you .await them, so this isn't
// actually making a HTTP request yet.
let async_fn = reqwest::get("http://adamchalmers.com");

// Wrap the async function in my hypothetical wrapper.
let timed_async_fn = TimedWrapper::new(async_fn);

// Call the async function, which will send a HTTP request and time it.
let (resp, time) = timed_async_fn.await;
println!("Got a HTTP {} in {}ms", resp.unwrap().status(), time.as_millis())
```

这个接口十分简单且便于团队中其他成员使用。让我们试着实现它吧！Rust 中的异步函数其实就是返回 [Future](https://doc.rust-lang.org/stable/std/future/trait.Future.html) 的函数。`Future`特征十分简单。它表示：

- 可以被轮询
- 当它被轮训时，它可能返回`Pending`或`Ready`
- 当它返回`Pending`时，应该在之后再次对其轮训
- 当它返回`Ready`时，它会携带一个值。

下面是一个`Future`的简单实现。

```rust
use std::{future::Future, pin::Pin, task::Context}

/// A future which returns a random number when it resolves.
#[derive(Default)]
struct RandFuture;

impl Future for RandFuture {
    // Every future has to specify what type of value it returns when it resolves.
    // This particular future will return a u16.
    type Output = u16;

    // The `Future` trait has only one method, named "poll".
    fn poll(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Self::Output> {
        Poll::ready(rand::random())
    }
}
```

不是很难！我觉得我们已经准备好实现`TimedWrapper`了。

## 尝试并使用嵌套 Future 失败

定义`TimedWrapper`类型

```rust
pub struct TimedWrapper<Fut: Future> {
    start: Option<Instant>,
    future: Fut,
}
```