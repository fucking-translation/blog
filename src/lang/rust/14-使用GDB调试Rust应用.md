# 使用 GDB 调试 Rust 应用

[原文](https://blog.logrocket.com/libp2p-tutorial-build-a-peer-to-peer-app-in-rust/)

![p2p](./img/Screen-Shot-2021-02-09-at-9.24.46-AM.webp)

</br>

根据你以前对编程语言和生态系统的了解，调试可能不是你从未做过的事情，或者说是你开发过程中绝对固定的流程。

举个例子，在 [Java](https://www.java.com/en/) ([Kotlin](https://kotlinlang.org/) 或者其他 JVM 语言) 生态系统中，由于其复杂工具的悠久历史，许多人(包括我自己)在正常的开发周期中都依赖调试器。在许多动态类型的语言中，这个工作流未被广泛的采用。

当然，这些是概括。几乎每种编程语言都具有某种调试机制，但是开发人员是否使用调试器似乎取决于工具的质量和可用性以及他们正在从事的任务。

无论如何，拥有良好的调试能力是开发过程中的关键部分。在这篇 Rust GDB 教程中，我们将会向你展示如何使用最棒的 Rust 调试工具：[GNU Project Debugger (GDB)](https://www.gnu.org/software/gdb/) 来调试 Rust 应用程序。


我们将涵盖以下内容：

- [GDB是什么](#GDB-是什么)
- [在 Rust 中设置 GDB](#在-Rust-中设置-GDB)
- [`rust-gdb`是什么](#`rust-gdb`是什么？)
- [`rust-gdb`示例](#rust-gdb示例)
- [布局和检查状态](#布局和检查状态)
- [操作状态和观察点](#操作状态和观察点)
- [调试一个异步网络程序](#调试一个异步网络程序)


## GDB 是什么

GNU Project Debugger (GDB) 是 [Richard Stallman](https://stallman.org/) 编写的极其古老的程序，它在 1986 年自称是“GNU 项目的首席 GNUisance”。GDB 支持多种语言，例如 C/C++ 以及现代语言如 [GO 和 Rust](https://blog.logrocket.com/when-to-use-rust-and-when-to-use-golang/)。

GDB 是一个命令行应用程序，但是有很多的 GUI 界面以及 IDE 将它进行了集成。举个例子，一个现代的，基于浏览器的实现是 [gdbgui](https://www.gdbgui.com/)。在本篇教程中，我们将使用它的命令行接口，因为它可以在任何地方运行，不需要外部依赖，并且足够简单，可以用于我们要完成的工作。

GDB 可以运行在 Linux，MacOS 以及 Windows 中，并且大多数 Linux 的发行版预装了 GDB。你可以阅读 [GDB文档](https://www.gnu.org/software/gdb/documentation/)以获取平台的安装说明。

GDB 十分复杂且功能强大，因此在本教程中我们不会深入探讨 GDB。我们将使用它最基本的功能，如设置断点，运行程序，逐步执行，打印变量等等。

## 在 Rust 中设置 GDB

为了继续本文以下内容，你需要安装最新版的 Rust (1.39+) 以及最新版的 GDB (8.x+)。可能也需要一个 TCP 包的发送工具，比如：`netcat`。

同样，你需要确保在`rustc`的同级目录中有`rust-gdb`可执行程序。如果你使用 [Rustup](https://rustup.rs/) 安装并更新 Rust，这应该默认就存在的。

首先，创建一个新的 Rust 项目：

```console
cargo new rust-gdb-example
cd rust-gdb-example
```

接下来，编辑`Cargo.toml`文件并添加你需要的依赖。

```toml
[dependencies]
tokio = { version = "1.1", features=["full"] }
```

在这里，我们只添加 Tokio 依赖，因为我们将构建一个非常基础的异步 TCP 示例来演示我们可以像调试“普通函数”那样调试异步函数。

在`src/lib.rs`中添加以下代码：

```rust
#[derive(Clone, Debug)]
pub enum AnimalType {
    Cat,
    Dog,
}

#[derive(Clone, Debug)]
pub struct Animal {
    pub kind: AnimalType,
    pub name: String,
    pub age: usize,
}

#[derive(Clone, Debug)]
pub struct Person {
    pub name: String,
    pub pets: Vec<Animal>,
    pub age: usize,
}
```

这些只是我们将在示例程序中调试的基础类型。

## `rust-gdb`是什么？

`rust-gdb`是 Rust (如：使用 Rustup) 安装时附带的预构建二进制文件，且会自动安装。

基本上，`rust-gdb`是将外部 Python 的 pretty-printing 脚本加载到 GDB 中的封装。在调试更加复杂的 Rust 程序时将很有用(并且在一定程度上是必需的)，因为它可以显著改善 Rust 数据类型的显示。

举个例子，带有 pretty-printing 的`Vec<Animal>`代码如下所示：

```rust
Vec(size=3) = {rust_gdb_example::Animal {kind: rust_gdb_example::AnimalType::Cat, name: "Chip", age: 4}, rust_gdb_example::Animal {kind: rust_gdb_example::AnimalType::Cat, name: "Nacho", age: 6}, rust_gdb_example::Animal {kind: rust_gdb_example::AnimalType::Dog, name: "Taco", age: 2}}
```

不带 pretty-printing 的代码如下所示：

```rust
alloc::vec::Vec<rust_gdb_example::Animal> {buf: alloc::raw_vec::RawVec<rust_gdb_example::Animal, alloc::alloc::Global> {ptr: core::ptr::unique::Unique<rust_gdb_example::Animal> {pointer: 0x5555555a1480, _marker: core::marker::PhantomData<rust_gdb_example::Animal>}, cap: 3, alloc: alloc::alloc::Global}, len: 3}
```

pretty-printing 脚本为大多数广泛使用的 Rust 结构如`Vec`，`Option`，`Result`等提供了格式化，隐藏了它们的内部信息并展示了实际的 Rust 类型 - 这是我们在大多数时间都会感兴趣的内容。

这也是此时涉及 Rust 的调试方法的明显限制之一。如果你有复杂的嵌套的数据类型，你将需要知道它们的内部信息，或者使用某种黑魔法来正确的检查值。随着时间的流逝，这种情况会有所改善，但是从目前的情况来看，如果你使用这种方法调试复杂的实际软件，将会遇到问题。

在不进行设置的情况下，我们从一个示例程序开始，并使用它启动`rust-gdb`。

## `rust-gdb`示例

让我们从一个如何在 Rust 中使用 GDB 的基本示例开始。

在你的项目中创建`examples`文件夹并添加带有以下内容的`basic.rs`文件：

```rust
use rust_gdb_example::*;

fn main() {
    let animals: Vec<Animal> = vec![
        Animal {
            kind: AnimalType::Cat,
            name: "Chip".to_string(),
            age: 4,
        },
        Animal {
            kind: AnimalType::Cat,
            name: "Nacho".to_string(),
            age: 6,
        },
        Animal {
            kind: AnimalType::Dog,
            name: "Taco".to_string(),
            age: 2,
        },
    ];

    get_chip(&animals);
}

fn get_chip(animals: &Vec<Animal>) {
    let chip = animals.get(0);

    println!("chip: {:?}", chip);
}
```