# libp2p 教程：使用 Rust 构建 p2p 应用

[原文](https://blog.logrocket.com/libp2p-tutorial-build-a-peer-to-peer-app-in-rust/)

![p2p](./img/macro_in_rust.webp)

</br>

在过去几年中，由于围绕区块链和加密货币 (cryptocurrencies) 的火热炒作 (hype)，去中心化应用的发展势头风光一时无两。人们对去中心化的兴趣日益浓厚的另一个因素是，人们越来越意识到在数据隐私与垄断 (monopolization) 方面，将大多数网络信息交到一小部分公司手中的弊端 (downside)。

不管怎样，除了 (aside from) 所有的加密与区块链技术外，最近在去中心化软件领域出现了一些非常有趣的进展。

值得关注的包括 [IPFS](https://ipfs.io/)；全新的分布式编码平台 [Radicle](https://radicle.xyz/)；去中心化社交网络 [Scuttlebutt](https://scuttlebutt.nz/)；以及 [Fediverse](https://fediverse.party/) 中的其他应用程序，如 [Mastodon](https://joinmastodon.org/)。

在这篇教程中，我们将会向你展示如何使用 Rust 和 [libp2p](https://github.com/libp2p/rust-libp2p) 来构建一个非常简单的 p2p 应用程序。其中 [libp2p](https://github.com/libp2p/rust-libp2p) 是一个非常棒的库，对于不同语言，它处于不同的成熟阶段。

我们将使用简单的命令行界面构建一个烹饪食谱应用程序，使我们能够：

- 创建食谱
- 发布食谱
- 列出本地食谱清单
- 列出我们在网络中发现的其他成员
- 列出指定成员发布的食谱
- 列出我们其他成员的所有食谱

我们将通过 300 行左右的 Rust 代码来实现所有上述功能。让我们开始吧！

## 安装 Rust

为了继续以下内容，你需要的只是安装最新的 Rust 版本 (1.47+)。

首先，创建一个新的 Rust 工程：

```console
cargo new rust-p2p-example
cd rust-p2p-example
```

然后，编辑`Cargo.toml`文件并添加你所需要的依赖：

```toml
[dependencies]
libp2p = { version = "0.31", features = ["tcp-tokio", "mdns-tokio"] }
tokio = { version = "0.3", features = ["io-util", "io-std", "stream", "macros", "rt", "rt-multi-thread", "fs", "time", "sync"] }
serde = {version = "=1.0", features = ["derive"] }
serde_json = "1.0"
once_cell = "1.5"
log = "0.4"
pretty_env_logger = "0.4"
```

就像上面所说的一样，我们将会使用 [libp2p](https://github.com/libp2p/rust-libp2p) 来开发 p2p 应用的网络部分。更值得一提的是，我们将使其与 tokio 异步运行时配合使用 (use it in concert with)。我们将使用 Serde 作为 Json 的序列化与反序列化器，以及其他用于日志打印以及初始化状态的帮助类库。

## 什么是 `libp2p`？

[libp2p](https://libp2p.io/) 是一个专注于模块化构建 p2p 应用的协议簇。

[libp2p](https://libp2p.io/) 有多语言的实现类库，如：JavaScript，Go 以及 Rust。这些库都实现了相同的`libp2p`规范，因此，一个 Go 语言的`libp2p`客户端可以无缝的与 JavaScript 实现的另一个客户端进行交互，只要它们在选择的协议栈方面兼容即可。