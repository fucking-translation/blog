# 使用 nom 解析 url

#### [原文](https://blog.logrocket.com/parsing-in-rust-with-nom/) 

</br>

![nom](./img/parsing-rust-nom.png)

</br>

在本教程中，我们将演示如何使用 nom 解析器组合器库在 Rust 中编写一个非常基础的 URL 解析器。我们将包含一下内容

- [什么是解析器组合器?](#什么是解析器组合器?)
- [nom是如何工作的](#nom是如何工作的)
- [设置nom](#设置nom)
- [数据类型](#数据类型)
- [nom中的错误处理](#nom中的错误处理)
- [在Rust中写一个解析器](#在Rust中写一个解析器)
- [解析需要授权的URL](#解析需要授权的URL)
- [Rust解析:Host,Ip和端口](#Rust解析:Host,Ip和端口)
- [在Rust中解析路径](#在Rust中解析路径)
- [查询和片段](#查询和片段)
- [在Rust中使用nom解析:最终的测试](#在Rust中使用nom解析:最终的测试)

## 什么是解析器组合器?

解析器组合器是高阶函数，可以接受多个解析器作为输入，并返回一个新的解析器作为输出。

这种方式让你可以为简单的任务(如：解析某个字符串或数字)构建解析器，并使用组合器函数将它们组合成一个递归下降(recursive descent)的解析器。

组合解析的好处包括可测试性，可维护性和可读性。每个部件都非常小且具有自我隔离性，从而使整个解析器由模块化组件构成。

如果你对这个概念不熟悉，我强烈推荐你阅读 Bodil Stokke 的[用 Rust 学习解析器组合器](./01-用Rust学习解析器组合器.md)。

## nom是如何工作的

[nom](https://github.com/Geal/nom) 是使用 Rust 编写的解析器组合器库，它可以让你创建安全的解析器，而不会占用内存或影响性能。它依靠 Rust 强大的类型系统和内存安全来生成既正确又高效的解析器，并使用函数，宏和特征来抽象出容易出错的管道。

为了演示 `nom` 是如何工作的，我们将创建一个基础的 URL 解析器。我们不会完整的实现 [URL 规范](https://url.spec.whatwg.org/)；这将远远超出此代码示例的范围。相反，我们将采用一些捷径。

最终的目标是能够将合法的 URL (如：[https://www.zupzup.org/about/?someVal=5&anotherVal=hello#anchor](https://www.zupzup.org/about/?someVal=5&anotherVal=hello#anchor) 和 [http://user:pw@127.0.0.1:8080](http://user:pw@127.0.0.1:8080)) 解析成相关的结构，并在解析过程中为非法的 URL 返回一个有用的错误。

而且，由于可测试性被认为是解析器组合器的一大优势，我们将对大多数组件进行测试，以了解其具体的优势。

让我们开始吧！

## 设置nom

为了进行下面的一系列操作，你需要安装最新的 Rust 版本 (1.44+)。

首先，创建一个新的 Rust 项目:

```console
cargo new --lib rust-nom-example
cd rust-nom-example
```

然后，编辑`Cargo.toml`文件并添加你需要的依赖：

```toml
[dependencies]
nom = "6.0"
```

是的，我们需要的是最新版本的`nom`库(在撰写本文时是 6.0)。

## 数据类型

编写解析器时，通常先定义输出结构以了解你需要哪些部分是很有意义的。

在这里，我们正在解析一个 URL，因此，让我们给它定义一个结构：

```rust
#[derive(Debug, PartialEq, Eq)]
pub struct URI<'a> {
    scheme: Scheme,
    authority: Option<Authority<'a>>,
    host: Host,
    port: Option<u16>,
    path: Option<Vec<&'a str>>,
    query: Option<QueryParams<'a>>,
    fragment: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Scheme {
    HTTP,
    HTTPS,
}

pub type Authority<'a> = (&'a str, Option<&'a str>);

#[derive(Debug, PartialEq, Eq)]
pub enum Host {
    HOST(String),
    IP([u8; 4]),
}

pub type QueryParam<'a> = (&'a str, &'a str);
pub type QueryParams<'a> = Vec<QueryParam<'a>>;
```

让我们逐行进行说明。

这些字段是根据它们在常规 URI 中出现的顺序进行排列的。首先，我们有 scheme。在这里，我们将 URI 的前缀限制为`http://`和`https://`，但是请注意，这里还有很多其它可选的 scheme。

接下来是`authority`部分，它由用户名和可选密码组成，通常是完全可选的。

host 可以是 IP，(在我们的示例中仅为 IPv4)，也可以是主机字符串，如：`example.org`，后面跟一个可选的port，port 仅是个数字：如：`localhost:8080`。

在端口之后是 path。它是由`/`分割的字符串序列，如：`/some/important/path`。query 和 fragment 部分是可选的，它们表示 URL 的`?query=some-value&another=5`和`#anchor`部分。query 是字符串元组的可选列表，而 fragment 只是可选字符串(完整的 URL 示例是`https://some/important/?query=some-value&another=5#anchor`)。

如果你对这些类型中的生命周期(`'a`)感到困惑，请不用感到沮丧；它不会真的影响到我们写代码的方式。本质上，我们可以使用指向输入字符串各部分的指针，而不是为 URL 的每一部分分配新的字符串，只要输入的生命周期和我们 URI 结构一样长就可以了。

在开始解析之前，让我们实现`From`特征将合法的 scheme 转换成`Scheme`枚举：

```rust
impl From<&str> for Scheme {
    fn from(i: &str) -> Self {
        match i.to_lowercase().as_str() {
            "http://" => Scheme::HTTP,
            "https://" => Scheme::HTTPS,
            _ => unimplemented!("no other schemes supported"),
        }
    }
}
```

顺便说一句，让我们从顶部开始，开始解析 scheme。

## nom中的错误处理

在我们开始之前，先讨论一下 `nom` 中的错误处理。虽然我们不会面面俱到，但是至少会让调用者大致了解在解析的哪一步出了什么问题。

为了达到我们的目的，我们将使用`nom`中的`context`组合器。在`nom`中，一个解析器通常会返回如下类型：

```rust
type IResult<I, O, E = (I, ErrorKind)> = Result<(I, O), Err<E>>;
```

在本例中，我们将返回一个输入值(`&str` - 输入字符串)的元组类型。它包含仍需要解析的字符串，以及输出的值。当解析失败时，它也会返回一个错误。

标准的`IResult`只允许我们使用 nom 内置的错误类型，如果我们想要创建自定义的错误类型以及在这些错误中添加一些上下文呢？

`ParserError` 特征和 `VerboseError` 类型让我们可以构建自己的错误类型，并可以在已有的错误中添加上下文。在这个简单的例子中，我们将会在我们的解析错误类型中添加上下文。为了方便起见，让我们定义一个自己的结果类型。

```rust
type Res<T, U> = IResult<T, U, VerboseError<T>>;
```