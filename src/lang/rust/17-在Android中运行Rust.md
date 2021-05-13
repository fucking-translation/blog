# 在 Android 中运行 Rust 

[原文](https://blog.svgames.pl/article/running-rust-on-android)

为了我目前的一位客户，我们决定将 Rust 作为我们主要的编程语言。做出这个决定的原因有很多：除了技术优势 (merit) 外，还有一个无可争议的 (undisputable) 事实就是：Rust 仍然是一门相对较新的语言，花哨 (fancy) 且时髦 (hip) - 当你是一家初创公司 (startup) 时，使用十几年前的技术可能会让你陷入困境。我的意思是，这是合乎逻辑的 - 不使用创新的技术如何进行创新？最快的成功方式就是对其大肆宣传 (aboard the hype train)。

”用户持有自己的数据“应该是产品的一个卖点，它不能是一个完全通过浏览器访问的服务，而应该是一种可以分发给用户，并让其运行在用户设备上的某个东西。我们在内部已经运行了一些 headless (一种不需要外设的模式) 实例，只要再完成一些琐碎的 (trivial) 工作，就可以为 Windows 和 Linux 系统制作可重新分发的程序包。但是我们知道如果程序包只能运行在桌面操作系统中时，将会严重阻碍应用的普及 - 如果我们想让它脱颖而出 (take off)，则需要该应用程序的移动版本。这意味着我们必须要知道如何让我们的程序运行在 Android 或者 iOS 系统中。因为我对交叉编译与自动化构建已经有了一些经验，我主动的研究了这个主题。

## 获取工具

先从基础开始，我需要获取 Rust 交叉编译器。幸运的是，Rust 让此操作变得十分简单，因为只需要调用以下命令：

```console
$ rustup target add armv7-linux-androideabi  # For 32-bit ARM.
$ rustup target add aarch64-linux-android    # For 64-bit ARM.

# x86_64 is mainly useful for running your app in the emulator.
# Speaking of hardware, there are some commercial x86-based tablets,
# and there's also hobbyists running Android-x86 on their laptops.
$ rustup target add x86_64-linux-android
```
*（注意：以后只会显示 aarch64 架构的所有示例）*

我还需要 Android 的构建工具。在经过一番调研之后，我来到 [Android Studio 的下载页面](https://developer.android.com/studio#command-tools) 并抓取了归档的命令行工具。尽管 SDK 包有 80+ MiB 大小，却依然只有所需工具的最小子集，所以我听从了互联网的建议并使用`sdkmanager`来安装额外的部件。

```console
$ cd ~/android/sdk/cmdline-tools/bin/
$ ./sdkmanager --sdk_root="${HOME}/android/sdk" --install 'build-tools;29.0.2'
$ ./sdkmanager --sdk_root="${HOME}/android/sdk" --install 'cmdline-tools;latest'
$ ./sdkmanager --sdk_root="${HOME}/android/sdk" --install 'platform-tools'
$ ./sdkmanager --sdk_root="${HOME}/android/sdk" --install 'platforms;android-29'
```

尽管 Android 支持运行原生 (native) 代码，但是大多数应用还是采用 Java 或者 Kotlin 来编写，SDK 反应了这一点。为了能够使用原生代码，我还需要一个工具 - 原生开发工具套件 (Native Development kit)。[NDK 下载页面](https://developer.android.com/ndk/downloads) 提供了几个版本以供选择 - 在经过一段深思熟虑后，我决定使用 LTS 版本：r21e。

## 足够简单！或想太多？

搞定了开发工具之后，我决定试着直接编译项目。

```console
$ cargo build --target=aarch64-linux-android
```

和预期的一样，构建失败了，并且错误信息占满了整个屏幕。经过筛选 (sift) 后，显示存在一个链接错误：

```console
error: linking with `cc` failed: exit code: 1
/usr/bin/ld: startup.48656c6c6f20546865726521.o: Relocations in generic ELF (EM: 183)
/usr/bin/ld: startup.48656c6c6f20546865726521.o: error adding symbols: file in wrong format
collect2: error: ld returned 1 exit status
```

我认为这(错误提示)足够简单 - Cargo 试图使用系统的链接器 (linker) 而不是 Android NDK 的链接器。我可以使用`CC`和`LD`环境变量让 Cargo 指向正确的链接器。

```console
$ export ANDROID_NDK_ROOT="${HOME}/android/ndk"
$ export TOOLCHAIN="${ANDROID_NDK_ROOT}/toolchains/llvm/prebuilt/linux-x86_64"
$ export CC="${TOOLCHAIN}/bin/aarch64-linux-android29-clang"
$ export LD="${TOOLCHAIN}/bin/aarch64-linux-android-ld"
$ cargo build --target=aarch64-linux-android
```

让我失望的是，这并没有起作用。我不愿意花费一天的时间来和 Cargo 纠缠 (wrestle)，因此我决定寻找是否有其他人给出了解决方案 - 很快，我便找到看似十分完美的工具。

## cargo-apk

[cargo-apk](https://crates.io/crates/cargo-apk) 是一个可以简单的将 Cargo 项目构建成`.apk`的工具。你所需要做得就是安装这个工具，在`Cargo.toml`文件中添加一些配置，然后你就可以继续了。

```toml
# cargo-apk compiles your code to an .so file,
# which is then loaded by the Android runtime
[lib]
path = "src/main.rs"
crate-type = ["cdylib"]
 
# Android-specic configuration follows.
[package.metadata.android]
# Name of your APK as shown in the app drawer and in the app switcher
apk_label = "Hip Startup"
 
# The target Android API level.
target_sdk_version = 29
min_sdk_version = 26
 
# See: https://developer.android.com/guide/topics/manifest/activity-element#screen
orientation = "portrait"
```

有了上面添加的配置，我试图使用`cargo-apk`来构建项目。

```console
$ cargo install cargo-apk
$ export ANDROID_SDK_ROOT="${HOME}/android/sdk"
$ export ANDROID_NDK_ROOT="${HOME}/android/ndk"
$ cargo apk build --target aarch64-linux-android
```

令人惊奇的是，它成功了！(等等) 额，好吧，我再一次遇到了链接错误。但是这一次，它不是关于重定位和文件格式的神秘错误，而是一个缺失库的简单情况：

```console
error: linking with `aarch64-linux-android29-clang` failed: exit code: 1
    aarch64-linux-android/bin/ld: cannot find -lsqlite3
    clang: error: linker command failed with exit code 1 (use -v to see invocation)
```

## 依赖，依赖，依赖

我们的项目使用 [SQLite](https://sqlite.org/)，这是一个 C 库。尽管 Rust 社区在每个可能的场合都吹捧 (tout) ”用 Rust 重写“在某种程度上是臭名昭著的，但是实际上与流行库一起使用的 crate 并不需要重新实现，因为这需要大量的 (colossal) 工作。相反，它们仅提供在 Rust 代码中调用库的方式，既可以作为 C 函数重新导出，也可以提供更加友好的 API 并稍微抽象化 FFI 调用。我们使用的 [rusqlite](https://crates.io/crates/rusqlite) 并没有什么不同，意味着我们也需要构建 SQLite。

SQLite 使用 GNU Autotool 进行构建。在对环境变量和用于配置的选项有了一些了解之后，我仔细浏览了 NDK 的文档 - 我找到了一个在各种构建系统([包括 Autotools](https://developer.android.com/ndk/guides/other_build_systems#autoconf)) 中使用 NDK 的文档页面。尽管 Google 提供了 LTS 版本的 NDK，以及最新版本的文档，但在 r21 LTS 和最新的 r22 之间发生了变化，事情变得稍微有点棘手。幸运的是，Wayback 机器具有该页面的[历史版本](http://web.archive.org/web/20200531051836/https://developer.android.com/ndk/guides/other_build_systems#autoconf)，让我能够找到合适的 NDK r21 的说明。

```console
$ ANDROID_API=29
$ TOOLCHAIN="${ANDROID_NDK_ROOT}/toolchains/llvm/prebuilt/linux-x86_64"i
$ export CC="${TOOLCHAIN}/bin/aarch64-linux-android${ANDROID_API}-clang"
$ export CXX="${TOOLCHAIN}/bin/aarch64-linux-android${ANDROID_API}-clang++"
$ export AR="${TOOLCHAIN}/bin/aarch64-linux-android-ar"
$ export AS="${TOOLCHAIN}/bin/aarch64-linux-android-as"
$ export LD="${TOOLCHAIN}/bin/aarch64-linux-android-ld"
$ export RANLIB="${TOOLCHAIN}/bin/aarch64-linux-android-ranlib"
$ export STRIP="${TOOLCHAIN}/bin/aarch64-linux-android-strip"
$ ./configure --host=aarch64-linux-android --with-pic
$ make -j $(nproc)
```

## 接住我，Scotty

使用上述方法，成功构建了 SQLite，生成了`libsqlite3.so`。现在只需要知道如何让 Cargo 使用它即可。在浏览 Cargo Book 时，我遇到了讲述[环境变量](https://doc.rust-lang.org/cargo/reference/environment-variables.html)的一个章节，它提及了`RUSTFLAGS`。和 Make 或 CMake 对待`CFLAGS`和`CXXFLAGS`一样，`RUSTFLAGS`的内容被 Cargo 传递给`rustc`编译器，允许它影响编译器的行为。

尽管这种方式十分简单，但是对我来说不是很优雅，因此我进一步深入研究了其他选项。继续浏览 Cargo Book，我遇到了描述项目配置的章节，可以肯定的是，[有一种方法可以指定 RUSTFLAGS](https://doc.rust-lang.org/cargo/reference/config.html#buildrustflags)。然而，无论我如何尝试，我始终都会收到来自 Cargo 的提示，告诉我关于未使用的 manifest 键的信息。

```console
warning: unused manifest key: target.aarch64-linux-android.rustflags
```

浏览 Cargo Book 的更多章节，我遇到了关于[构建脚本](https://doc.rust-lang.org/cargo/reference/build-scripts.html)的章节。它们毫无疑问是一个强大的工具，但是我已经花费了很多时间摸索 (fumble) 学习 Cargo 的配置，不想再花更多的时间阅读关于如何编写构建脚本的内容，因此，最终我选择了环境变量的解决方案，~~并且可能会在之后尝试使用构建脚本的方式~~(不可能)。

我在终端中输入命令，并焦急的观察它的执行过程。

```console
$ RUSTFLAGS="-L $(pwd)/sqlite-autoconf-3340000/.libs/" cargo apk build --target aarch64-linux-android
```

再一次，它。。。在某种程度上成功了。虽然链接器不再将错误解释成缺失库，但是`cargo-apk`依然无法找到该链接器并将其添加到最终的 APK 文件中。

```console
 'lib/arm64-v8a/libstartup.so'...
Shared library "libsqlite3.so" not found.
Verifying alignment of target/debug/apk/statup.apk (4)...
      49 AndroidManifest.xml (OK - compressed)
     997 lib/arm64-v8a/libstartup.so (OK - compressed)
Verification succesful
```