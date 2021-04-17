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

[libp2p](https://libp2p.io/) 有多语言的实现类库，如：JavaScript，Go 以及 Rust。这些库都实现了相同的`libp2p`规范，因此，一个 Go 语言的`libp2p`客户端可以无缝的与 JavaScript 实现的另一个客户端进行交互，只要它们在选择的协议栈方面兼容即可。这些协议涵盖了从基本网络传输协议到安全层协议以及多路复用的广泛范围。

在本文中我们不会深入讲解`libp2p`的细节，但是如果你想要更深入的学习，[libp2p 官方文档](https://docs.libp2p.io/concepts/)将很好地概述我们在此过程中会遇到的各种概念。

## `libp2p` 是如何工作的

 为了查看`libp2p`的实际效果，我们将从定义一些需要的常量和类型开始构建食谱应用。

 ```rust
 const STORAGE_FILE_PATH: &str = "./recipes.json";

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

static KEYS: Lazy<identity::Keypair> = Lazy::new(|| identity::Keypair::generate_ed25519());
static PEER_ID: Lazy<PeerId> = Lazy::new(|| PeerId::from(KEYS.public()));
static TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("recipes"));
 ```

我们将在名为`recipes.json`的 JSON 文件中存储本地食谱，应用的可执行程序将与其放在同一目录下。我们将定义一个名为`Result`的帮助类型，它将有助于传播任意的错误。

然后，我们使用`once_cell::Lazy`来懒加载一些东西。首要的是，我们使用它来生成密钥对，并从公钥中派生所谓的 (so-called) `PeerId`。我们还创建了一个`Topic`，它是`libp2p`中另一个关键的概念。

这是什么意思呢？简而言之，在整个 p2p 网络中，`PeerId`是一个特定成员的唯一的标志符。我们从密钥对中派成它以确保它的唯一性。而且这个密钥对可以让我们与网络中其他的成员进行安全通信，确保没有人可以冒充 (impersonate) 我们。

另一方面，`Topic`是 Floodsub 中的概念，它实现了`libp2p`中的 [pub/sub](https://github.com/libp2p/specs/tree/master/pubsub) 接口。`Topic`是一种我们可以订阅并发送消息的组件 - 举个例子，只监听 `pub/sub` 网络中流量的子集。

我们需要为食谱定义一些类型：

```rust
type Recipes = Vec<Recipe>;

#[derive(Debug, Serialize, Deserialize)]
struct Recipe {
    id: usize,
    name: String,
    ingredients: String,
    instructions: String,
    public: bool,
}
```

以及一些我们想要发送的消息类型：

```rust
#[derive(Debug, Serialize, Deserialize)]
enum ListMode {
    ALL,
    One(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct ListRequest {
    mode: ListMode,
}

#[derive(Debug, Serialize, Deserialize)]
struct ListResponse {
    mode: ListMode,
    data: Recipes,
    receiver: String,
}

enum EventType {
    Response(ListResponse),
    Input(String),
}
```

这个食谱相当简单。它有一个 ID，一个名称，一些配料 (ingredient) 以及烹饪的方法。而且，我们还添加了一个`public`标记以便区分我们想要分享的食谱以及想要保留的食谱。

正如开头提到的，这里有两种方式可以拉到其他成员的食谱清单：拉取全部获取某一个成员的食谱清单，通过`ListMode`来表示。

`ListRequest`和`ListResponse`仅仅是`ListMode`的封装，以及使用它们的发送时间。

`EventType`枚举用来区分来自其他成员的响应以及我们自己的输入。稍后我们将介绍为什么这种差异很重要。

## 创建一个`libp2p`客户端

让我们开始编写 main 函数，以便在 p2p 网络中创建一个成员。

```rust
#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    info!("Peer Id: {}", PEER_ID.clone());
    let (response_sender, mut response_rcv) = mpsc::unbounded_channel();

    let auth_keys = Keypair::<X25519Spec>::new()
        .into_authentic(&KEYS)
        .expect("can create auth keys");
```

我们初始化日志并创建了一个异步`channel`来与应用的其他部分进行通信。稍后，我们将使用此`channel`将来自`libp2p`网络栈的响应发送回我们的应用程序以进行处理。

另外，我们为[Noise](https://noiseprotocol.org/)加密协议创建了一些授权密钥，这些密钥将用于保护网络中的流量。为了达到这个目的，我们创建了一个新的密钥对，然后使用`into_authentic`函数通过身份密钥对其进行签名。

下一步很重要并涉及`libp2p`的核心概念：创建所谓的`Transport`：

```rust
let transp = TokioTcpConfig::new()
    .upgrade(upgrade::Version::V1)
    .authenticate(NoiseConfig::xx(auth_keys).into_authenticated())
    .multiplex(mplex::MplexConfig::new())
    .boxed();
```

transport 是一个面向连接的与其他成员进行通信的网络协议蔟。在同一个应用程序中可能也会使用多个 transport - 举个例子，TCP/IP，Websocket 或者 UDP 同时针对不同的用例进行通信。

在本例中，我们将使用 Tokio 的异步TCP 作为基础。一旦 TCP 连接建立，为了通信安全，我们将使用`Noise`对其进行`upgrade`操作。一个基于 web 的示例是在 HTTP 之上使用 TLS 创建安全连接。

我们将使用`NoiseConfig::xx`的握手模式，这是唯一一个可以与其他`libp2p`应用交互的选项。

`libp2p`的好处是，我们可以编写一个 Rust 客户端，另一个编写 JavaScript 客户端，只要在两个语言版本的库中都实现了(相同的)协议，它们仍然可以轻松地进行通信。

最后，我们还对 transport 进行[多路复用](https://docs.libp2p.io/concepts/stream-multiplexing/)，它可以让我们在相同的 transport 上复用多个 substream 或者连接。

理论性的东西有点多！但是所有这些都可以在 [libp2p 文档](https://docs.libp2p.io/)中找到。这只是创建 p2p transport 众多方法中的其中一种。

下一个概念是`NetworkBehaviour`。这是实际上是`libp2p`中定义网络和所有成员逻辑的部分 - 举个例子，当接收到事件应该做什么以及应该发送什么事件。

```rust
let mut behaviour = RecipeBehaviour {
    floodsub: Floodsub::new(PEER_ID.clone()),
    mdns: TokioMdns::new().expect("can create mdns"),
    response_sender,
};

behaviour.floodsub.subscribe(TOPIC.clone());
```

在这里，正如上面所提及的，我们将使用`FloodSub`协议处理这些事件。我们也使用 [mDNS](https://tools.ietf.org/html/rfc6762)，这是一种在本地网络中发现其他成员的协议。我们还会在这放置 channel 中的`sender`，以便可以使用它将事件传播到应用程序的主要模块中。

现在，我们已经从 behavior 中订阅了在之前创建的`FloodSub`主题，这意味着我们将接收并可以发送该主题的事件。

我们几乎已经完成了`libp2p`的设置。我们需要了解的最后一个概念是`Swarm`。

```rust
let mut swarm = SwarmBuilder::new(transp, behaviour, PEER_ID.clone())
    .executor(Box::new(|fut| {
        tokio::spawn(fut);
    }))
    .build();
```

[Swarm](https://docs.rs/libp2p/latest/libp2p/index.html#swarm) 管理使用 transport 创建的连接，并执行我们创建的网络行为(如：触发或接收事件)，并为我们提供一种从外部获取它们的方法。

我们使用 transport，behavior和 PEER_ID 创建了`Swarm`。`执行器`告诉`Swarm`使用`Tokio`运行时在内部运行，但是我们也可以在这里使用其他的异步运行时。

剩下的唯一一件事就是启动我们的`Swarm`：

```rust
Swarm::listen_on(
    &mut swarm,
    "/ip4/0.0.0.0/tcp/0"
    .parse()
    .expect("can get a local socket"))
    .expect("swarm can be started");
```

与 TCP 服务器类似，我们仅使用本地 IP 调用`listen_on`，让操作系统为我们确定端口。这将以之前的所有设置来启动`Swarm`，但实际上我们还没有定义任何逻辑。

让我们从处理用户输入开始。

## 在`libp2p`中处理用户输入

对于用户输入，我们仅仅依赖 STDIN。因此在调用`Swarm::listen_on`之前，我们将会添加：

```rust
let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();
```

它在 STDIN 之上定义了一个异步 reader，它可以逐行读取字节流。如果我们按下 enter 键，这里将会传入一个新的消息。

下一步我们将创建事件循环，它将会监听来自 STDIN，`Swarm`以及在上面定义的响应 channel 中的事件。

```rust
loop {
    let evt = {
        tokio::select! {
            line = stdin.next_line() => Some(EventType::Input(line.expect("can get line").expect("can read line from stdin"))),
            event = swarm.next() => {
                info!("Unhandled Swarm Event: {:?}", event);
                None
            },
            response = response_rcv.recv() => Some(EventType::Response(response.expect("response exists"))),
        }
    };
    ...
}
```

我们使用 Tokio 的`select`宏来等待几种异步流程，并处理第一个完成的流程。对于`Swarm`事件我们不做任何处理；这些事件都在之后将看到的`RecipeBehaviour`中进行处理，但我们仍需要调用`swarm.next()`以驱动`Swarm`转发。

让我们添加一些事件处理逻辑来取代上面的`...`：

```rust
if let Some(event) = evt {
    match event {
        EventType::Response(resp) => {
            ...
        }
        EventType::Input(line) => match line.as_str() {
            "ls p" => handle_list_peers(&mut swarm).await,
            cmd if cmd.starts_with("ls r") => handle_list_recipes(cmd, &mut swarm).await,
            cmd if cmd.starts_with("create r") => handle_create_recipe(cmd).await,
            cmd if cmd.starts_with("publish r") => handle_publish_recipe(cmd).await,
            _ => error!("unknown command"),
        },
    }
}
```

如果 evt 是一个事件，我们将会对其进行匹配并判断是否是`Response`还是`Input`事件。我们现在先看下`Input`事件。

- `ls p` 列出所有的已知成员
- `ls r` 列出所有的本地食谱
- `ls r {peerId}` 列出指定成员发布的食谱
- `ls r all` 列出所有已知成员发布的食谱
- `publish r {recipeId}` 发布指定的食谱
- `create r {recipeName}|{recipeIngredients}|{recipeInstructions}` 通过给定的数据以及自增的 ID 创建一个新的食谱

在这里，列出成员的所有食谱意味着要向所有成员发送一个关于食谱的请求并等待它们的响应，然后展示结果。在 p2p 网络中，这可能要花一点事件因为某些成员可能会在地球的另一端，我们甚至不知道它们是否会对我们进行响应。这和发送一个请求到 HTTP 服务器不一样，举个例子：

先看下列出成员清单的逻辑：

```rust
async fn handle_list_peers(swarm: &mut Swarm<RecipeBehaviour>) {
    info!("Discovered Peers:");
    let nodes = swarm.mdns.discovered_nodes();
    let mut unique_peers = HashSet::new();
    for peer in nodes {
        unique_peers.insert(peer);
    }
    unique_peers.iter().for_each(|p| info!("{}", p));
}
```

在这里，我们可以使用`mDNS`来发现所有的节点，遍历并展示它们。

接下来，让我们创建并发布食谱，在处理 (tackle) 列表命令之前：

```rust
async fn handle_create_recipe(cmd: &str) {
    if let Some(rest) = cmd.strip_prefix("create r") {
        let elements: Vec<&str> = rest.split("|").collect();
        if elements.len() < 3 {
            info!("too few arguments - Format: name|ingredients|instructions");
        } else {
            let name = elements.get(0).expect("name is there");
            let ingredients = elements.get(1).expect("ingredients is there");
            let instructions = elements.get(2).expect("instructions is there");
            if let Err(e) = create_new_recipe(name, ingredients, instructions).await {
                error!("error creating recipe: {}", e);
            };
        }
    }
}

async fn handle_publish_recipe(cmd: &str) {
    if let Some(rest) = cmd.strip_prefix("publish r") {
        match rest.trim().parse::<usize>() {
            Ok(id) => {
                if let Err(e) = publish_recipe(id).await {
                    info!("error publishing recipe with id {}, {}", id, e)
                } else {
                    info!("Published Recipe with id: {}", id);
                }
            }
            Err(e) => error!("invalid id: {}, {}", rest.trim(), e),
        };
    }
}
```

在这两种情况下，我们都需要解析字符串以获取被`|`分隔的数据，如果是`public`，则需要解析给定的食谱 id，如果给定的输入不合法，则打印一下错误日志。

对于`create`，我们通过给定的数据调用`create_new_recipe`函数。让我们查看一下与食谱的本地 JSON 存储交互所需的所有辅助函数。

```rust
async fn create_new_recipe(name: &str, ingredients: &str, instructions: &str) -> Result<()> {
    let mut local_recipes = read_local_recipes().await?;
    let new_id = match local_recipes.iter().max_by_key(|r| r.id) {
        Some(v) => v.id + 1,
        None => 0,
    };
    local_recipes.push(Recipe {
        id: new_id,
        name: name.to_owned(),
        ingredients: ingredients.to_owned(),
        instructions: instructions.to_owned(),
        public: false,
    });
    write_local_recipes(&local_recipes).await?;

    info!("Created recipe:");
    info!("Name: {}", name);
    info!("Ingredients: {}", ingredients);
    info!("Instructions:: {}", instructions);

    Ok(())
}

async fn publish_recipe(id: usize) -> Result<()> {
    let mut local_recipes = read_local_recipes().await?;
    local_recipes
        .iter_mut()
        .filter(|r| r.id == id)
        .for_each(|r| r.public = true);
    write_local_recipes(&local_recipes).await?;
    Ok(())
}

async fn read_local_recipes() -> Result<Recipes> {
    let content = fs::read(STORAGE_FILE_PATH).await?;
    let result = serde_json::from_slice(&content)?;
    Ok(result)
}

async fn write_local_recipes(recipes: &Recipes) -> Result<()> {
    let json = serde_json::to_string(&recipes)?;
    fs::write(STORAGE_FILE_PATH, &json).await?;
    Ok(())
}
```

最基本的构造代码块是`read_local_recipes`以及`write_local_recipes`，它们仅从存储中读取并反序列化食谱，以及序列化食谱并将其写入本地存储。

`publish_recipe`函数从文件中获取所有的食谱，通过给定的 ID 来查询食谱，并将它的`public`标记设置为 true。

当创建一个食谱时，我们也会从文件中读取所有的食谱，并在最后添加新的食谱，然后将全部数据写回并覆盖原文件。这不是很高效，但是它足够简单且可行。

## 使用`libp2p`发送消息

接下来让我们看下`list`命令，并探索如何将消息发送给其他成员。

在`list`命令中，这里可能有三种情况：

```rust
async fn handle_list_recipes(cmd: &str, swarm: &mut Swarm<RecipeBehaviour>) {
    let rest = cmd.strip_prefix("ls r ");
    match rest {
        Some("all") => {
            let req = ListRequest {
                mode: ListMode::ALL,
            };
            let json = serde_json::to_string(&req).expect("can jsonify request");
            swarm.floodsub.publish(TOPIC.clone(), json.as_bytes());
        }
        Some(recipes_peer_id) => {
            let req = ListRequest {
                mode: ListMode::One(recipes_peer_id.to_owned()),
            };
            let json = serde_json::to_string(&req).expect("can jsonify request");
            swarm.floodsub.publish(TOPIC.clone(), json.as_bytes());
        }
        None => {
            match read_local_recipes().await {
                Ok(v) => {
                    info!("Local Recipes ({})", v.len());
                    v.iter().for_each(|r| info!("{:?}", r));
                }
                Err(e) => error!("error fetching local recipes: {}", e),
            };
        }
    };
}
```

我们解析输入的命令，剥离 (strip) `ls r`部分，并检查是否还有剩余的部分命令。如果没有，我们仅读取本地的食谱并使用在之前定义的辅助函数将它们打印出来。

如果我们遇到了`all`关键字，我们将创建一个带有`ListMode::ALL`集合的`ListRequest`，将其序列化成 JSON，并在`Swarm`中使用`FloodSub`实例将其发布到之前提到的`主题`中。

如果我们在命令中遇到成员 ID，我们将仅发送带有该成员 ID 的`ListMode::One`。我们可以检查它是否是一个合法的成员 ID，或者是否是一个我们已经发现的成员 ID，但是为了保持简单：如果其上没有任何监听，则不会做任何处理。

这就是我们要向网络中发送消息所需要的一切。现在的问题是，这些消息会发生什么？它们在哪里被处理？

在本例中的 p2p 应用中，请记住我们既是事件的`Sender`也是事件的`Receiver`，因此在我们的实现中，需要处理输入以及响应事件。

## 使用`libp2p`对消息进行响应

我们的`RecipeBehaviour`终于在这里出现了。先对其进行定义：

```rust
#[derive(NetworkBehaviour)]
struct RecipeBehaviour {
    floodsub: Floodsub,
    mdns: TokioMdns,
    #[behaviour(ignore)]
    response_sender: mpsc::UnboundedSender<ListResponse>,
}
```

behavior 本身仅仅是一个结构体，但是我们使用了`libp2p`中的`NetworkBehaviour`派生宏，因此我们不需要手动实现特征的所有函数。

这个派生宏为结构体中所有未声明`behaviour(ignore)`的成员实现了 [NetworkBehaviour](https://docs.rs/libp2p/latest/libp2p/swarm/trait.NetworkBehaviour.html) 特征。在这里我们忽略了 channel，因为它与我们的 behavior 没有直接的关系。

接下来就是为`FloodsubEvent`和`MdnsEvent`实现`jnject_event`函数。

先从`mDNS`开始：

```rust
impl NetworkBehaviourEventProcess<MdnsEvent> for RecipeBehaviour {
    fn inject_event(&mut self, event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(discovered_list) => {
                for (peer, _addr) in discovered_list {
                    self.floodsub.add_node_to_partial_view(peer);
                }
            }
            MdnsEvent::Expired(expired_list) => {
                for (peer, _addr) in expired_list {
                    if !self.mdns.has_node(&peer) {
                        self.floodsub.remove_node_from_partial_view(&peer);
                    }
                }
            }
        }
    }
}
```

当这个处理器接收到一个事件时，将会调用`inject_event`函数。对于`mDNS`来说，这里只有两种事件`Discovered`和`Expired`，它们将在当我们在网络中发现一个新的成员或者一个成员离开时被触发。在这两种情况下，我们都可以在`FloodSub`的部分视图中将其添加或移除，该视图是要将消息传播到的节点列表。

对于 pub/sub 事件来说，`inject_event`有点复杂。我们需要对传入的`ListRequest`和`ListResponse`有效负载做出反应。如果我们发送了`ListRequest`，成员将会接收到一个请求，该请求会拉取它本地发布的食谱，并将其返回。

将它们返回给请求成员的唯一方式就是在网络中发布它们的食谱。由于 pub/sub 是我们唯一的机制，因此我们需要对传入的请求以及响应做出反应。

让我们来看它是如何工作的：

```rust
impl NetworkBehaviourEventProcess<FloodsubEvent> for RecipeBehaviour {
    fn inject_event(&mut self, event: FloodsubEvent) {
        match event {
            FloodsubEvent::Message(msg) => {
                if let Ok(resp) = serde_json::from_slice::<ListResponse>(&msg.data) {
                    if resp.receiver == PEER_ID.to_string() {
                        info!("Response from {}:", msg.source);
                        resp.data.iter().for_each(|r| info!("{:?}", r));
                    }
                } else if let Ok(req) = serde_json::from_slice::<ListRequest>(&msg.data) {
                    match req.mode {
                        ListMode::ALL => {
                            info!("Received ALL req: {:?} from {:?}", req, msg.source);
                            respond_with_public_recipes(
                                self.response_sender.clone(),
                                msg.source.to_string(),
                            );
                        }
                        ListMode::One(ref peer_id) => {
                            if peer_id == &PEER_ID.to_string() {
                                info!("Received req: {:?} from {:?}", req, msg.source);
                                respond_with_public_recipes(
                                    self.response_sender.clone(),
                                    msg.source.to_string(),
                                );
                            }
                        }
                    }
                }
            }
            _ => (),
        }
    }
}
```

我们匹配了传入的消息，并试图将其序列化成一个请求或响应。当我们接收到一个响应时，我们仅将带有调用者成员 ID 的响应打印出来，成员 ID 可以通过`msg.source`获取。当我们接收到一个请求时，我们需要区分`ALL`和`One`这两种情况。

对于`One`，我们检查给定的成员 ID 是否和我们一样 - 该请求其实是针对我们的。如果是，我们将返回发布的食谱，我们同样对`ALL`这样进行响应。

在这两种情况下，我们都将调用`respond_with_public_recipes`辅助函数：

```rust
fn respond_with_public_recipes(sender: mpsc::UnboundedSender<ListResponse>, receiver: String) {
    tokio::spawn(async move {
        match read_local_recipes().await {
            Ok(recipes) => {
                let resp = ListResponse {
                    mode: ListMode::ALL,
                    receiver,
                    data: recipes.into_iter().filter(|r| r.public).collect(),
                };
                if let Err(e) = sender.send(resp) {
                    error!("error sending response via channel, {}", e);
                }
            }
            Err(e) => error!("error fetching local recipes to answer ALL request, {}", e),
        }
    });
}
```

在这个辅助函数中，我们使用 Tokio 生成器异步执行 Future，该 Future 将读取所有的本地食谱，并以此创建一个`ListResponse`，然后通过`channel_sender`将数据发送给我们的事件循环，在事件循环中，我们将会这样处理：

```rust
EventType::Response(resp) => {
    let json = serde_json::to_string(&resp).expect("can jsonify response");
    swarm.floodsub.publish(TOPIC.clone(), json.as_bytes());
}
```

如果我们注意到“内部”通过`Response`事件发送，我们将其序列化成 JSON 格式并将其发送到网络中。

## 使用`libp2p`进行测试

上面那些是实现过程，现在让我们对其进行测试。

为了检查我们的实现是否工作，让我们在多个命令行中使用以下命令启动应用程序：

```console
RUST_LOG=info cargo run
```

请注意，应用程序需要一个同级目录下的`recipes.json`文件。

当应用启动时，我们获取以下日志，并打印出成员 ID：

```console
INFO  rust_peer_to_peer_example > Peer Id: 12D3KooWDc1FDabQzpntvZRWeDZUL351gJRy3F4E8VN5Gx2pBCU2
```

现在，我们需要输入 enter 来启动事件循环。

输入`ls p`之后，我们获取到了发现的成员列表：

```console
ls p
 INFO  rust_peer_to_peer_example > Discovered Peers:
 INFO  rust_peer_to_peer_example > 12D3KooWCK6X7mFk9HeWw69WF1ueWa3XmphZ2Mu7ZHvEECj5rrhG
 INFO  rust_peer_to_peer_example > 12D3KooWLGN85pv5XTDALGX5M6tRgQtUGMWXWasWQD6oJjMcEENA
```

使用 `ls r`，我们将获取到本地食谱：

```console
ls r
 INFO  rust_peer_to_peer_example > Local Recipes (3)
 INFO  rust_peer_to_peer_example > Recipe { id: 0, name: " Coffee", ingredients: "Coffee", instructions: "Make Coffee", public: true }
 INFO  rust_peer_to_peer_example > Recipe { id: 1, name: " Tea", ingredients: "Tea, Water", instructions: "Boil Water, add tea", public: false }
 INFO  rust_peer_to_peer_example > Recipe { id: 2, name: " Carrot Cake", ingredients: "Carrots, Cake", instructions: "Make Carrot Cake", public: true }
```

调用`ls r all`将向其他所有成员发送一个请求，并返回它们的食谱：

```console
ls r all
 INFO  rust_peer_to_peer_example > Response from 12D3KooWCK6X7mFk9HeWw69WF1ueWa3XmphZ2Mu7ZHvEECj5rrhG:
 INFO  rust_peer_to_peer_example > Recipe { id: 0, name: " Coffee", ingredients: "Coffee", instructions: "Make Coffee", public: true }
 INFO  rust_peer_to_peer_example > Recipe { id: 2, name: " Carrot Cake", ingredients: "Carrots, Cake", instructions: "Make Carrot Cake", public: true }
```

如果我们使用带有成员 ID 的`ls r`命令，将会发生同样的事情：

```console
ls r 12D3KooWCK6X7mFk9HeWw69WF1ueWa3XmphZ2Mu7ZHvEECj5rrhG
 INFO  rust_peer_to_peer_example > Response from 12D3KooWCK6X7mFk9HeWw69WF1ueWa3XmphZ2Mu7ZHvEECj5rrhG:
 INFO  rust_peer_to_peer_example > Recipe { id: 0, name: " Coffee", ingredients: "Coffee", instructions: "Make Coffee", public: true }
 INFO  rust_peer_to_peer_example > Recipe { id: 2, name: " Carrot Cake", ingredients: "Carrots, Cake", instructions: "Make Carrot Cake", public: true }
```

它确实有用！你也可以在同样的网络中使用更多的客户端进行尝试。

你可以在 [Github](https://github.com/zupzup/rust-peer-to-peer-example) 中获取本教程的完整代码示例。

## 结论

在本文中我们介绍了如何使用 Rust 以及`libp2p`构建一个简单的，去中心化的应用程序。

如果你来自 web 后端，那么你对许多的网络概念可能都很熟悉，但是构建 p2p 应用程序仍然需要一种根本不同的设计和构建方法。

`libp2p`类库已经十分成熟，并且由于 Rust 在加密领域十分流行，因此出现了一个新兴的 (emerge)，丰富的生态系统，用于构建功能强大的去中心化应用程序。