# std::sync::Condvar

[原文](https://doc.rust-lang.org/std/sync/struct.Condvar.html)

定义：一种条件变量

## 描述

条件变量代表阻塞线程的能力，以使线程在等待时间发生时不占用CPU时间。条件变量通常与布尔谓词(predicate)和互斥锁相关联，在验证线程必须阻塞之前，始终在互斥对象内部验证该谓词。

该模块中的函数将阻止当前执行的线程，并在可能的情况下绑定系统提供的条件变量。请注意，此模块对系统条件变量又一个附加限制：每一个`Condvar`在运行时可以与一个`Mutex`一起使用。任何在同一条件变量上使用多个互斥锁的尝试都将引起运行时`panic`。如果不希望这样，在`sys`中的`unsafe`原语将没有这种限制，但是可能会导致为定义的行为。

```rust
use std::sync::{Arc, Mutex, Condvar};
use std::thread;

let pair = Arc::new((Mutex::new(false), Condvar::new()));
let pair2 = pair.clone();

// Inside of our lock, spawn a new thread, and then wait for it to start.
thread::spawn(move|| {
    let (lock, cvar) = &*pair2;
    let mut started = lock.lock().unwrap();
    *started = true;
    // We notify the condvar that the value has changed.
    println!("111");
    cvar.notify_one();
});

// Wait for the thread to start up.
let (lock, cvar) = &*pair;
let mut started = lock.lock().unwrap();
println!("222");
while !*started {
    println!("333");
    started = cvar.wait(started).unwrap();
    println!("444");
}
```

输出

```
222
333
111
444
```

## 实现

### impl Condvar

> pub fn new() -> Condvar

创建一个新的条件变量，并随时等待并通知它。

```rust
use std::sync::Condvar;
let condvar = Condvar::new();
```

> pub fn wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> LockResult<MutexGuard<'a, T>>

阻塞当前线程，直到条件变量接收到一个通知。

这个函数将会自动解锁指定的`MutexGuard`并且阻塞当前线程。这意味着在互斥体(`mutex`)解锁后，任何逻辑上的调用`notify_one`和`notify_all`方法都是唤醒该线程的候选对象。当函数返回时，将重新获得指定的锁。

请注意，这个方法易受虚假唤醒的影响。条件变量通常使用布尔谓词和他们建立关联，并且该方法每次返回时都必须检查谓词以防止虚假唤醒。

❗️：如果正在等待的互斥锁在此线程重新获取锁时中毒，该方法将返回错误。想要了解更多信息，请参阅有关资料: 中毒的互斥体(`Mutex`)。

😱：如果同时使用多个互斥体将会触发`panic!`。每个条件变量都动态的绑定在一个互斥体上以确保跨平台行为定义。如果不需要此限制，则需提供`sys`中的`unsafe`原语。

```rust
use std::sync::{Arc, Mutex, Condvar};
use std::thread;

let pair = Arc::new((Mutex::new(false), Condvar::new()));
let pair2 = pair.clone();

thread::spawn(move|| {
    let (lock, cvar) = &*pair2;
    let mut started = lock.lock().unwrap();
    *started = true;
    // We notify the condvar that the value has changed.
    cvar.notify_one();
});

// Wait for the thread to start up.
let (lock, cvar) = &*pair;
let mut started = lock.lock().unwrap();
// As long as the value inside the `Mutex<bool>` is `false`, we wait.
while !*started {
    started = cvar.wait(started).unwrap();
}
```

> pub fn wait_while<'a, T, F>(&self, guard: MutexGuard<'a, T>, condition: F) -> LockResult<MutexGuard<'a, T>> where F: FnMut(&mut T) -> bool

阻塞当前线程直到条件变量收到一个通知并且所提供的条件为`false`为止。

这个函数将会自动解锁指定的`MutexGuard`并且阻塞当前线程。这意味着在互斥体(`mutex`)解锁后，任何逻辑上的调用`notify_one`和`notify_all`方法都是唤醒该线程的候选对象。当函数返回时，将重新获得指定的锁。

❗️：如果正在等待的互斥锁在此线程重新获取锁时中毒，该方法将返回错误。想要了解更多信息，请参阅有关资料: 中毒的互斥体(`Mutex`)。

```rust
use std::sync::{Arc, Mutex, Condvar};
use std::thread;

let pair = Arc::new((Mutex::new(true), Condvar::new()));
let pair2 = pair.clone();

thread::spawn(move|| {
    let (lock, cvar) = &*pair2;
    let mut pending = lock.lock().unwrap();
    *pending = false;
    // We notify the condvar that the value has changed.
    cvar.notify_one();
});

// Wait for the thread to start up.
let (lock, cvar) = &*pair;
// As long as the value inside the `Mutex<bool>` is `true`, we wait.
let _guard = cvar.wait_while(lock.lock().unwrap(), |pending| { *pending }).unwrap();
```

> pub fn wait_timeout<'a, T>(&self, guard: MutexGuard<'a, T>, dur: Duration) -> LockResult<(MutexGuard<'a, T>, WaitTimeoutResult)>

在此条件变量上等待通知，在指定的持续时间之后超时。

<font color='yellow'>此函数的语义等同于`wait`（除了线程被阻塞的时间不超过`dur`），由于诸如抢占或平台差异之类的异常可能不会导致等待的最大时间精确地变短，因此该方法不应用于精确的计时。</font>

⚠️：已尽最大的努力确保等待时间是由单调时间测量的，并且不会被系统时间修改所影响。这个函数容易受到虚假唤醒的影响。条件变量通常都会有一个布尔谓词