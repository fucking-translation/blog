# Rust 中的类型强转

[原文](https://www.possiblerust.com/guide/what-can-coerce-and-where-in-rust)

Rust 支持多种[类型强制转换](https://doc.rust-lang.org/reference/type-coercions.html)，它可以隐式的一种类型转换成另一种类型。与其他支持类型强转的语言一样，Rust 在易读性与易写性之间做了权衡。虽然对于 Rust 支持类型强转的清单是否最好存在分歧，但是学习类型转换是有意义的，因为有些是惯用的 (idiomatic) Rust 代码的核心。在本文中，我将描述 Rust 支持什么样的类型转换，以及在何处应用。

## 什么是(类型)强转

在讨论类型强转之前，最好先弄清楚它的含义。Rust 支持多种类型转换的方式。`From`和`Into`特性用于库级别的可靠 (infallible) 转换。`TryFrom`和`TryInto`用于处理易出错的类型转换。`AsRef`，`AsMut`，`Borrow`和`ToOwned`提供了更多不同类型之间库级转换。但是，这些都是显式的。要执行转换，用户必须调用相关的函数。相比之下，强转是隐式的，这些转换的隐式属性意味着它们仅在其裨益依赖于易用性时才是可用的，并且隐式类型更改造成的潜在危害最小。使用`as`关键字完成的强转是显式的，并且允许的显式强转 (cast) 比隐式强转 (coercion) 要多。

> **INFO 1** ，`transmute` - unsafe 转换  
> 标准库中有一个函数`std::mem::transmute`，它可以将任意类型转换成其他类型。该函数是`unsafe`的，因为它不能保证输入类型的有效位可以表示为输出类型的有效位。确保这两种类型兼容由用户决定。  
>
> 有一个致力于在 Rust 中开发“safe transmute”选项的工作，可以称之为“Project Safe Transmute”。他们的工作正在进行中，目的是当讨论的转化合法时，不需要使用`unsafe`版本的`transmute`(意味着源类型的有效位始终是目标类型中的有效位)。

## 有哪些隐式的类型强转 (coercion) 呢？

Rust 支持多种隐式的类型强转，尽管它们的定义都是非正式的，但是仍然需要进行一定程度的标准化。事实上，这些转换的长期规范预计将成为最终标准化过程的一部分，因为它们对于理解 Rust 的类型系统至关重要。

> **INFO 2**，在标准化编程语言中  
>
>
>