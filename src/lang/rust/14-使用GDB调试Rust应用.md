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

GDB 是一个命令行应用程序，但是有很多的 GUI 界面以及 IDE 将它进行了集成。举个例子，一个现代的，基于浏览器的实现是 [gdbgui](https://www.gdbgui.com/)。在本篇教程中，我们将使用它的命令行界面，因为它可以在任何地方运行，不需要外部依赖，并且足够简单，可以用于我们要完成的工作。

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

让我们从一个在 Rust 中使用 GDB 的基本示例开始。

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

这个非常简单的程序初始化了一个 animals 列表，并在最后调用了一个函数，该函数打印了 animals 列表中第一个元素。

为了调试这个程序，我们需要将其构建并使用`rust-gdb`执行构建的二进制文件。请确保使用调试模式对其进行构建而不是发布模式。

```rust
cargo build --example basic
Finished dev [unoptimized + debuginfo] target(s) in 0.28s

rust-gdb target/debug/examples/basic
```

如果我们不是在构建示例，而是在构建二进制文件，二进制文件将在`target/debug`目录下。

在运行`rust-gdb`时，GDB 会打印几行欢迎信息和一个输入提示`(gdb)`。

如果你之前没有使用过 GDB，[GDB cheat sheet](https://darkdust.net/files/GDB%20Cheat%20Sheet.pdf) 将会对你有所帮助。

我们可以通过使用`break`命令或缩写`b`来设置一个断点：

```rust
(gdb) b get_chip
Breakpoint 1 at 0x13e3c: file examples/basic.rs, line 26.
(gdb) info b
Num     Type           Disp Enb Address            What
1       breakpoint     keep y   0x0000000000013e3c in basic::get_chip at examples/basic.rs:26
```

我们可以在某一行上设置断点(如：`basic.rs:17`)，或者在某个函数中设置断点。我们可以使用`info b`来查看断点，它向我们展示了断点的位置，数字(以便我们可以删除，禁用或启用它)，以及它是否被启用(`Enb`)。

`info`命令可以使用其他的选项，比如`info locals`，它展示了本地变量，`info args`，它显示了传入的函数参数以及更多的选项。

既然我们设置了断点，我们可以通过执行`run`(或`r`)来运行程序：

```rust
(gdb) r
Starting program: /home/zupzup/dev/oss/rust/rust-gdb-example/target/debug/examples/basic
[Thread debugging using libthread_db enabled]
Using host libthread_db library "/lib/x86_64-linux-gnu/libthread_db.so.1".

Breakpoint 1, basic::get_chip (animals=0x7fffffffd760) at examples/basic.rs:26
26            let chip = animals.get(0);
```

它启动了程序。我们停在了定义的断点上，位于`get_chip`函数的第一行。在这里，我们可以查看函数的参数并尝试打印它们。

```rust
(gdb) info args
animals = 0x7fffffffd760
(gdb) p animals
$1 = (*mut alloc::vec::Vec<rust_gdb_example::Animal>) 0x7fffffffd760
(gdb) p *animals
$2 = Vec(size=3) = {rust_gdb_example::Animal {kind: rust_gdb_example::AnimalType::Cat, name: "Chip", age: 4}, rust_gdb_example::Animal {kind: rust_gdb_example::AnimalType::Cat, name: "Nacho", age: 6}, rust_gdb_example::Animal {kind: rust_gdb_example::AnimalType::Dog, name: "Taco", age: 2}}
```

`info args`命令提供了传入参数的概览。当我们使用`p`(`print`同样有用)打印 animals时，GDB 告诉我们处理的是指向`Vec<Animal>`的指针，但是并没有向我们展示任何有关`Vec`内容的信息，因为它只是一个指针。

你也可以使用`display`来打印变量，并且这里有很多格式的选项(如：字符串，指针，整型等)。`print`和`display`的区别是，使用`display`，在每次逐步执行指令之后，都会再次打印该值。这对于监控值的变更将很有用。

我们需要使用`*animals`来解引用指针。如果我们将其打印，我们会获得完整的，可读的 animal 列表。基础指针的戏法 (juggle) 以及类型转换是我们在引用结构体时处处需要的东西。

OK，我们现在在哪儿？让我们执行`f`或`frame`来查看我们到底在哪里：

```rust
(gdb) f
#0  basic::get_chip (animals=0x7fffffffd760) at examples/basic.rs:26
26            let chip = animals.get(0);
```

好吧，在我们设置的第一个断点那里。如果只有一种方法可以以图形化的方式来查看我们在源代码中的位置...

## 布局和检查状态

GDB 中的`布局`可以帮助你查看你处于 Rust 代码中什么位置上。使用`layout src`命令打开一个命令行界面：

![Layout-GDB-SRC-Command-Line-Interface](./img/Layout-GDB-SRC-Command-Line-Interface.avif)

我们的命令行提示在它的右下方。使用这种方式，我们再也不需要疑惑我们处于代码中什么位置上了。这里还有其他的布局，如`layout split`，它展示了代码以及相应的汇编：

![GDB-Layout-Split-Visual](./img/GDB-Layout-Split-Visual.avif)

看起来十分简洁。如果你想要摆脱这样的布局，你可以使用`CTRL+X a`。如果界面的渲染变混乱了，使用`CTRL+L`将会刷新界面(这会时而触发)。

与其他调试器一样，我们可以使用`n`或者`next`逐行执行代码，或者使用`s`或者`step`跳入函数内部。如果你想重复这个操作，你可以简单的按下回车键，然后上一个命令就会重复执行了。

让我们再往下执行，并在调用`Vec<Animal>`的`.get`方法查看`chip`变量的内部是什么：

```rust
(gdb) n
28            println!("chip: {:?}", chip);
(gdb) p chip
$3 = core::option::Option<&rust_gdb_example::Animal>::Some(0x5555555a1480)
(gdb) print *(0x5555555a1480 as &rust_gdb_example::Animal)
$4 = rust_gdb_example::Animal {kind: rust_gdb_example::AnimalType::Cat, name: "Chip", age: 4}
```

我们执行`n`，现在我们处于下一行上 (28)。在这里，我们试图打印`chip`，然后我们看到它是一个`Option`类型，其中包含了一个`Animal`的引用。不幸的是，GDB 再一次只向我们展示了它的地址。我们需要将其转换成`&rust_gdb_example::Animal`，然后查看 animal 中真实的值。

一个很棒的事情就是大部分这些事情都是自动完成的。因此如果你键入`rust_gd`，并按下`TAB`键，这些都将自动完成。和`AnimalType`以及其他类型，函数，作用域中的变量一样。

我们也可以打印函数定义：

```rust
(gdb) p get_chip
$11 = {fn (*mut alloc::vec::Vec<rust_gdb_example::Animal>)} 0x555555569370 <basic::get_chip>
```

如果你想到这个函数的结尾处，我们可以使用`finish`，然后跳出并来到调用该函数的地方。如果我们使用当前的断点完成了某个调试，我们可以使用`continue`或者`c`来继续执行程序 - 在这里，将会简单的运行程序并到达它的结尾处。

```rust
(gdb) finish
Run till exit from #0  basic::get_chip (animals=0x7fffffffd760) at examples/basic.rs:28
chip: Some(Animal { kind: Cat, name: "Chip", age: 4 })
0x0000555555567d87 in basic::main () at examples/basic.rs:22
22            get_chip(&animals);
(gdb) c
Continuing.
[Inferior 1 (process 61203) exited normally]
```

这是不是很棒！这些都是你调试 Rust 程序的必要功能。让我们来查看另一个示例并探索更高级的技术。

## 操作状态和观察点

首先，让我们在`examples`文件夹下的`nested.rs`文件中创建另一个示例：

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

    let mut some_person = Person {
        name: "Some".to_string(),
        pets: animals,
        age: 24,
    };
    println!("person: {:?}", some_person);
    some_person.age = 100;
    some_person.name = some_func(&some_person.name);
}

fn some_func(name: &str) -> String {
    name.chars().rev().collect()
}
```

我们再一次创建了 animal 列表。但是这一次，我们也创建了`Person`并将 animals 设置为他们的宠物。而且，我们会打印 person，将他们的年龄设置为`100`并倒置他们的名字(这是 some_func 做得事情)。

在我们调试这个程序之前，我们需要再一次构建它，并使用`rust-gdb`执行二进制文件：

```console
cargo build --example nested
rust-gdb target/debug/examples/nested
```

感觉真不错。让我们在第 22 行和第 27 行设置断点并运行该程序：

```rust
(gdb) b nested.rs:22
Breakpoint 1 at 0x17abf: file examples/nested.rs, line 22.
(gdb) b nested.rs:27
Breakpoint 2 at 0x17b13: file examples/nested.rs, line 27.
(gdb) info b
Num     Type           Disp Enb Address            What
1       breakpoint     keep y   0x0000000000017abf in nested::main at examples/nested.rs:22
2       breakpoint     keep y   0x0000000000017b13 in nested::main at examples/nested.rs:27
(gdb) r
Starting program: /home/zupzup/dev/oss/rust/rust-gdb-example/target/debug/examples/nested
[Thread debugging using libthread_db enabled]
Using host libthread_db library "/lib/x86_64-linux-gnu/libthread_db.so.1".

Breakpoint 1, nested::main () at examples/nested.rs:22
22            let mut some_person = Person {
```

我们在第一个断点处，该位置创建了 person。让我们继续打印语句。接着，我们将在`some_person.age`设置所谓的观察点。这个观察点将会在`some_person.age`每一次改变的时候通知我们。

```rust
(gdb) c
(gdb) watch some_person.age
Hardware watchpoint 3: some_person.age
(gdb) n
person: Person { name: "Some", pets: [Animal { kind: Cat, name: "Chip", age: 4 }, Animal { kind: Cat, name: "Nacho", age: 6 }, Animal { kind: Dog, name: "Taco", age: 2 }], age: 24 }
28            some_person.age = 100;
(gdb) n

Hardware watchpoint 3: some_person.age

Old value = 24
New value = 100
0x000055555556bba8 in nested::main () at examples/nested.rs:28
28            some_person.age = 100;
```

GDB 向我们展示了哪一个观察点被触发，以及对应的新值和旧值。

让我们再一次通过调用`run`来重新运行该程序，并确认我们想要重新运行