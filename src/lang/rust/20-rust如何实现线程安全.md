# Rust 如何实现线程安全

在我迄今为止的每一次演讲中，都会遇到 “Rust 是如何实现线程安全”的提问，我通常只是概述一下，而本文为感兴趣的人提供了更全面的解释。

你也可以参阅：[Huon 关于此主题的博客](http://huonw.github.io/blog/2015/02/some-notes-on-send-and-sync/)

在[之前的文章](http://manishearth.github.io/blog/2015/05/27/wrapper-types-in-rust-choosing-your-guarantees/)中，我稍微谈到了 [Copy] 特征。标准库中还有其他这样的“标记”特征，本文中与之相关的是[Send] 和 [Sync] 特征。如果你对像 [RefCell] 和 [Rc] 这样的包装器类型不熟悉，我建议你阅读那篇文章，因为我将在本文中将它们作为示例；但是这里解释的概念在很大程度上是独立的。

处于本文的目的，我将线程安全限制为没有数据竞争或跨线程悬垂指针。Rust 的目的不是为了解决竞争条件。然而，有些项目利用类型系统来提供某种形式的额外安全，如 [rust-session](https://github.com/Munksgaard/rust-sessions) 尝试使用会话类型提供协议安全。

这些特征是使用被称为“可选的内置特征”自动实现的。举个例子，如果`struct Foo`仅包含 [Sync] 字段，则它也将是 [Sync]，除非我们使用`impl !Sync for Foo {}`明确说明不实现此特征。类似的，如果`struct Foo`包含至少一种非 [Sync] 类型，则它也不会是 [Sync]，除非它显式指定`unsafe impl Sync for Foo {}`。

这意味着，[Send] 类型的 [Sender] 本身就是 [Send]，但非 [Send] 类型的 [Sender] 将不是 [Send]。这种模式非常强大。它允许在单线程上下文中使用具有非线程安全数据的通道，而无需单独的“单线程”通道抽象。

同时，像 [Rc] 和 [RefCell] 这样包含 [Send]/[Sync] 字段的结构已经明确指出退出其中的一个或多个，因为它们依赖的不变量 (invariants) 在多线程的情况下不成立。

实际上可以在编译器之外设计你自己的具有类似线程安全保证的库 - 虽然这些标记特征由编译器特别处理，但它们的工作不需要特殊处理。这里可以使用任意两个可选 (opt-in) 的内置特性。

[Send] 和 [Sync] 的含义略有不同，但是功能有交织 (intertwined) 的部分。

[Send] 类型可以在线程之间移动而不会有问题。它回答了“如果某个变量被移动到另一个线程，仍然可以使用吗”的问题。大多数完全拥有其包含数据的对象都符合此要求。值得注意的是，[Rc] 没有(因为它是共享所有权)。另一个例外是[LocalKey]，它确实拥有自己的数据，但对其他线程无效。借用的数据确实有资格被发送，但在大多数情况下，由于稍后会涉及的约束，它不能跨线程发送。

即使像 [RefCell] 这样的类型使用非原子引用计数，它也可以在线程之间安全的发送，因为这是所有权的转移 (move)。将 [RefCell] 发送到另一个线程将是一个 move，并且将无法在原来的线程中使用。

另一方面，[Sync] 与同步访问有关。它回答了“如果多个线程都试图访问这些数据，它会安全吗？”。[Mutex] 等类型和其他基于`lock/atomic`的类型以及原始类型都实现了这一点。包含指针的结构通常不是 [Sync]。

[Sync] 有点像 [Send] 的拐杖 (crutch)。它有助于在涉及共享时让其他类型具有 [Send] 特征。例如，`&T`和 [Arc<T>] 仅在内部数据为 [Sync] 时才具有 [Send] 特征(在 [Arc<T>] 的情况下有一个额外的 [Send] 边界)。换句话说，如果`共享/借用 (shared/borrowed)`的数据是同步安全 (synchronous-safe) 的，则可以将具有`共享/借用`所有权的内容发送到另一个线程。

由于非原子引用计数，[RefCell] 是 [Send] 但不是 [Sync]。

把它们放在一起，所有这些的看门人 (gatekeeper) 是 [thread::spawn()]。它有签名：

```rust
pub fn spawn<F, T>(f: F) -> JoinHandle<T> 
where 
    F: FnOnce() -> T, 
    F: Send + 'static, 
    T: Send + 'static
```

诚然，这令人感到困惑，部分原因是它允许返回一个值，还返回了一个句柄，我们可以从中阻塞线程连接 (thread join)。不过，我们可以为我们的需要创造一个更简单的`spawn`API：

```rust
pub fn spawn<F>(f: F) 
where 
    F: FnOnce(), 
    F: Send + 'static
```

可以这样调用：

```rust
let mut x = vec![1,2,3,4];

// `move` instructs the closure to move out of its environment
thread::spawn(move || {
   x.push(1);

});

// x is not accessible here since it was moved
```

`spawn()` 将接受一个将被调用一次的可调用对象(通常是一个闭包)，并包含 [Send] 和`'static`的数据。这里，`'static`只是意味着闭包中不包含借用的数据。这是前面提到的阻止跨线程共享借用数据的约束。如果没有它，我们将能够将借用的指针发送到一个线程，该线程很容易超过借用时间，从而导致安全问题。

这里有一个关于闭包的细微差别 - 闭包可以捕获外部变量，但默认情况下它们是通过引用进行的(因此有`move`关键字)。它们根据捕获子句自动实现 [Send] 和 [Sync]。有关它们内部的更多信息，请参阅 [huon 的博客](http://huonw.github.io/blog/2015/05/finding-closure-in-rust/)。在这种情况下，`x`将被捕获；即作为 [Vec<T>] (而不是类似于`&Vec<T>`或其他东西)，所以闭包本身可以是 [Send]。如果没有`move`关键字，闭包就不会是`'static`，因为它包含借用的内容。

由于闭包继承了其捕获数据的`Send/Sync/'static`，捕获正确类型数据的闭包将满足`F: Send + 'static`边界。

此函数允许和不允许的一些示例(对于`x`类型)：

- [Vec<T>]，[Box<T>] 是允许的，因为它们是 [Send] 和`'static`(当内部类型是相同类型时)。
- `&T`是不允许的，因为它不是`'static`的。这很棒，因为借用应该有一个静态已知的生命周期。将借用的指针发送到其他线程可能会导致释放后使用，或者以其他方式破坏别名规则 (aliasing rules)。
- `Rc<T>`不是`Send`，所以是不允许的。我们可能会有其他一些`Rc<T>`闲置，并最终导致引用计数上的数据竞争。
- `Arc<Vec<u32>>`是允许的(如果内部类型`Vec<T>`是`Send`和`Sync`)；我们不能在这里造成安全违规。迭代器失效需要可变性，而`Arc<T>`默认不提供。
- `Arc<Cell<T>>`是不允许的。`Cell<T>`提供基于复制的内部可变性，并且不是`Sync`(因此`Arc<Cell<T>>`不是`Send`)。如果允许这样做，我们可能会遇到较大的结构同时从不同的线程写入的情况，从而导致两者随机混杂，即数据竞争。
- `Arc<Mutex<T>>`或`Arc<RwLock<T>>`是允许的(对于`Send T`)。内部类型使用线程安全锁并提供基于锁的内部可变性。它们可以保证在任何时间点只有一个线程正在写入。因此，互斥体 (mutex) 是`Sync`，只要其内部 T 是`Send`即可，`Sync`类型可以与`Arc`等包装器安全的共享。从内部类型的角度来看，它一次只能被一个线程访问(RwLock 的情况稍微复杂一点)，因此不需要知道所涉及的线程。当涉及这些`Sync`类型时，就不会出现数据竞争。

如上所述，你实际上可以创建一对非`Send`对象的`Sender/Receiver`。这听起来有点违反直觉 (counterintuitive) - 我们不是应该只发送`Send`的值吗？但是`Sender<T>`仅当`T`为`Send`时才为`Send`；所以即使我们可以使用非`Send`类型的`Sender`，我们也不能将它发送到另一个线程，因此它不能用于破坏线程安全。

还有一种方法可以讲`&T`的`Send`用于某些`Sync T`，即`thread::scoped`。这个函数没有`'static`边界，但它有一个`RAII`保护，它在借用结束之前强制 join。这使得不需要互斥体 (Mutex) 就可以轻松的实现 fork-join 并行性。可悲的是，当这与`Rc`循环交互时会出现问题，因此该 API 目前不稳定，将会重新设计。这不是语言设计或`Send/Sync`设计的问题，而是库中小设计的不一致导致的完美风暴。

[Copy]: http://doc.rust-lang.org/std/marker/trait.Copy.html
[Send]: http://doc.rust-lang.org/std/marker/trait.Send.html
[Sync]: http://doc.rust-lang.org/std/marker/trait.Sync.html
[Rc]: https://doc.rust-lang.org/std/rc/struct.Rc.html
[Rc<T>]: https://doc.rust-lang.org/std/rc/struct.Rc.html
[RefCell]: https://doc.rust-lang.org/std/cell/struct.RefCell.html
[Sender]: http://doc.rust-lang.org/std/sync/mpsc/struct.Sender.html
[Sender<T>]: http://doc.rust-lang.org/std/sync/mpsc/struct.Sender.html
[LocalKey]: https://doc.rust-lang.org/nightly/std/thread/struct.LocalKey.html
[Mutex]: http://doc.rust-lang.org/std/sync/struct.Mutex.html
[Arc<T>]: https://doc.rust-lang.org/std/sync/struct.Arc.html
[thread::spawn()]: http://doc.rust-lang.org/std/thread/fn.spawn.html
[Box<T>]: http://doc.rust-lang.org/std/boxed/struct.Box.html
[RwLock]: http://doc.rust-lang.org/std/sync/struct.RwLock.html
[Receiver]:http://doc.rust-lang.org/std/sync/mpsc/struct.Receiver.html
[thread::scoped()]: http://doc.rust-lang.org/std/thread/fn.scoped.html
[`Vec<T>`]: https://doc.rust-lang.org/std/vec/struct.Vec.html