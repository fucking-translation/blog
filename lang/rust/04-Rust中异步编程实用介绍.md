# Rust 中异步编程实用介绍

[原文](http://jamesmcm.github.io/blog/2020/05/06/a-practical-introduction-to-async-programming-in-rust/)

> 在本文中，我们将探讨一个使用 Tokio 运行时在 Rust 中进行异步编程的简短示例，展示了不同的执行场景。这篇文章主要是针对异步编程的初学者的。

这个示例的代码可以在 [Github](https://github.com/jamesmcm/async-rust-example) 中获取，也可以使用基于`async-std`运行时的分支(由 [@BartMassey](https://github.com/BartMassey) 贡献)。

## 什么是异步编程

异步编程可以让你在等待 I/O 操作(通常是网络请求或响应)结果的同时，即使在单个 OS 线程中，也可以继续执行计算。

这是通过使用异步运行时来实现的，该运行时将异步任务(即：[绿色线程](https://en.wikipedia.org/wiki/Green_threads))分配给实际的 OS 线程。

与 OS 线程不同，创建绿色线程并不昂贵，因此我们不必担心是否达到了硬件限制。而 OS 线程需要维护自己的堆栈，因此在处理多个线程时会占用大量内存。在 Linux 中你可以使用`cat /proc/sys/kernel/threads-max`命令来查看每个进程的线程数限制，我的是 127162。

例如，如果我们需要一个单独的 OS 线程来处理 Web 服务器上的每个请求，这将是一个重大的问题，这是 [C10k 问题](https://en.wikipedia.org/wiki/C10k_problem)的根源 - 如何处理 Web 服务器上的 10000 个连接。

早期的 Web 服务器确实为每个请求分配了独立的 OS 线程，以便并行处理每个请求。但是这会造成这些线程花费了大量的时间来等待网络响应，而不是做其他的计算。

## Async 和 await

