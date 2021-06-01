# 开放容器倡议 (OCI) 规范

[原文](https://alibaba-cloud.medium.com/open-container-initiative-oci-specifications-375b96658f55)

OCI (Open Container Initiative) 是一个由行业合作提出的倡议，旨在定义有关容器镜像格式与运行时的开放[容器](https://www.alibabacloud.com/product/container-service)规范。就开源世界的合作与竞争而言，从最初的分歧到如何一步步到达今天地位上的历史是一个非常有趣的故事。

如今，容器生态系统的所有主要玩家都遵循 OCI 容器规范。对于任何想要了解容器工作原理的同学，这是一门你绝不想错过的技术。

## 概览

OCI 有两项规范，[Image spec](https://github.com/opencontainers/image-spec) 和 [Runtime spec](https://github.com/opencontainers/runtime-spec)。

下图显示了它们涵盖的内容和交互的方式。

![oci1](./img/oci1.png)

OCI 镜像可以从其他地方下载(如 Docker Hub) 并在 OCI 运行时文件系统包中解压。然后 OCI 运行时包将会运行在 OCI 运行时中。其中，OCI 运行时规范定义了如何运行一个”文件系统包“。

## 镜像规范 (image-spec)

镜像规范定义了 OCI 容器镜像的格式，其中包含 manifest，镜像索引 (image index)，一组文件系统层以及一个配置文件。这个规范的目标是创建互操作工具，用于构建，传输，以及准备要运行的容器镜像。

在顶层视角，容器镜像只是个 tarball 压缩包，在解压之后，它拥有以下`布局`。

```console
├── blobs
│   └── sha256
│       ├── 4297f0*      (image.manifest)
│       └── 7ea049       (image.config)
├── index.json
└── oci-layout
```

如果没有说明这些东西都是些什么以及它们是如何关联的，该布局就没那么有用。

我们可以简单的忽略`oci-layout`文件。`index.json`是入口点，它包含了主要的`manifest`，其中列出了所有用于单个容器镜像的”资源“。

`manifest`包含主要的`config`和`layers`。

将它使用图表表示：

![oci2](./img/oci2.png)

[配置 (config)](https://github.com/opencontainers/image-spec/blob/master/config.md) 包含：
- 镜像的配置，它将会转换成 OCI 运行时包的运行时配置文件。
- layers，构成 OCI 运行时包的根文件系统
- 一些与镜像历史相关的元数据

[层级 (layers)](https://github.com/opencontainers/image-spec/blob/master/layer.md) 构成最终的`rootfs`。第一层级是基础，其他所有层级仅包含第一次层级的变更。在下一节中我们深入研究一下什么是 OCI 层级规范。

## 层级

规范中本质上关于层级定义了以下两点：

1. 如何表示一个层级

  - 对于基础层级， 压缩所有的内容
  - 对于非基础层级，压缩所有与其主层级比较后变更内容的集合
  - 因此，首先检测变更，形成一个`changeset`；然后压缩此变更集合以表示该层级

2. 如何联合所有层级

在主层级的顶层应用所有的变更集合。将会给你带来`rootfs`。

## 运行时规范 (runtime-spec)

一旦在磁盘文件系统中将镜像解压到 OCI 运行时包中，你可以将其运行起来。此时是 OCI 运行时规范在起作用。该规范指定了容器的配置，执行环境以及生命周期。

容器配置包含必要的元数据用来创建及运行此容器。其中包含要使用的运行线程，环境变量，资源限制以及沙盒功能等。一些配置与平台无关，可用于 Linux，Windows，Solaris 以及特定的虚拟机；但另一些配置与平台相关，可能仅适用于 Linux。

OCI 运行时规范还定义了容器的生命周期，这是一系列从容器创建到停止时发生的事件。

## 容器生命周期

容器拥有生命周期，从本质上讲，它可以用以下状态图建模。

你可以添加一些其他的动作和状态，如`pause`和`paused`，但是这些都是最oci基础的。

![oci3](./img/oci3.png)

此状态图比较常规，但是这里有一个重要的事情需要提及一下 - `Hooks`。你可能有些惊讶，容器规范并没有定义如何设置网络，它实际上依赖 hook 来正确的设置网络，比如在容器启动之前创建网络并在容器停止之后删除它。

## 容器配置

我们之前提过容器的配置包含创建和运行容器的必要配置，我们将更仔细地查看一些配置，以了解容器的真正含义，并且我们将专注于 Linux 平台上的所有配置。

1. Root

    它定义了容器的根文件系统。

2. Mounts
    
    它指定了可以挂在到根文件系统上的附加文件系统。你在使用它挂载本地路径或者分布式路径(如 Ceph)。

3. Process

    它指定了与你要在容器中运行的进程相关的所有内容。包括环境变量和进程参数。

对于 Linux 进程，你可以额外指定有关进程安全方面的内容，如 capabilities，rlimits 以及 selinux 标签。

1. Hooks

    你可以在此处连接到容器的生命周期，并执行网络设置与清理操作。

2. Linux 命名空间

    
