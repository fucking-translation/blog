# 用Rust编写LLVM的玩具前端  

#### [原文](https://blog.ulysse.io/post/a-toy-front-end-for-llvm-written-in-rust/) 

</br>

我目前的副业是用Rust编写一个可以将代码转换成LLVM IR的编译器。LLVM的API对于新手（noobs）来说有点令人生畏（daunting），而且没有很多有关的教程（有限的教程大多数还是基于C++的，如何使用Rust做同样的事并不总是那么明显）。我希望当我准备做一件事情时，有人可以手把手的教我，这也是我要写这篇文章的原因。

对于Rust，与LLVM的接口交互的最佳选择是使用`llvm-sys`。互联网上的一些好心人在[这里](http://rustdoc.taricorp.net/llvm-sys/llvm_sys/)托管了一些关于`llvm-sys`的文档。当然，你还应该去查看LLVM的[官方指南](http://llvm.org/docs/tutorial/LangImpl01.html)，因为它可以帮助你理解LLVM是如何“思考”的。这篇文章基本上是LLVM[官方指南]的Rust翻译。

你可以从这里获取最终的[代码](https://github.com/ucarion/llvm-rust-getting-started)。

## 搭建开发环境