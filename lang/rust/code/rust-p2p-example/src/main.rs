use once_cell::sync::Lazy;
use libp2p::{PeerId, Transport, mplex, Swarm, NetworkBehaviour};
use libp2p::identity;
use libp2p::floodsub::{Topic, Floodsub, FloodsubEvent};
use libp2p::noise::{X25519Spec, Keypair, NoiseConfig};
use libp2p::tcp::TokioTcpConfig;
use libp2p::core::upgrade;
use libp2p::mdns::{TokioMdns, MdnsEvent};
use tokio::sync::mpsc;
use libp2p::swarm::{SwarmBuilder, NetworkBehaviourEventProcess};
use tokio::io::AsyncBufReadExt;
use std::collections::HashSet;
use serde::{Serialize, Deserialize};
use log::{error, info};

const STORAGE_FILE_PATH: &str = "./recipes.json";
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;
static KEYS: Lazy<identity::Keypair> = Lazy::new(|| identity::Keypair::generate_ed25519());
static PEER_ID: Lazy<PeerId> = Lazy::new(|| PeerId::from(KEYS.public()));
static TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("recipes"));

type Recipes = Vec<Recipe>;

#[derive(Debug, Serialize, Deserialize)]
struct Recipe {
    id: usize,
    name: String,
    ingredients: String,
    instructions: String,
    public: bool,
}

#[derive(Debug, Serialize, Deserialize)]
enum ListMode {
    ALL,
    One(String)
}

#[derive(Debug, Serialize, Deserialize)]
struct ListRequest {
    mode: ListMode
}

#[derive(Debug, Serialize, Deserialize)]
struct ListResponse {
    mode: ListMode,
    data: Recipes,
    receiver: String,
}

enum EventType {
    Response(ListResponse),
    Input(String)
}

#[derive(NetworkBehaviour)]
struct RecipeBehaviour {
    floodsub: Floodsub,
    mdns: TokioMdns,
    #[behaviour(ignore)]
    response_sender: mpsc::UnboundedSender<ListResponse>,
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    info!("Peer Id: {}", PEER_ID.clone());

    let (response_sender, mut response_rcv) = tokio::sync::mpsc::unbounded_channel();

    let auth_keys = Keypair::<X25519Spec>::new()
        .into_authentic(&KEYS)
        .expect("can create auth keys");

    let transport = TokioTcpConfig::new()
        .upgrade(upgrade::Version::V1)
        .authenticate(NoiseConfig::xx(auth_keys).into_authenticated())
        .multiplex(mplex::MplexConfig::new())
        .boxed();

    let mut behaviour = RecipeBehaviour {
        floodsub: Floodsub::new(PEER_ID.clone()),
        mdns: TokioMdns::new().expect("can create mdns"),
        response_sender
    };

    behaviour.floodsub.subscribe(TOPIC.clone());

    let mut swarm = SwarmBuilder::new(transport, behaviour, PEER_ID.clone())
        .executor(Box::new(|fut| {
            tokio::spawn(fut);
        })).build();

    Swarm::listen_on(
        &mut swarm,
        "/ip4/0.0.0.0/tcp/0"
            .parse()
            .expect("can get a local socket"))
            .expect("swarm can be started");

    let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();

    loop {
        let evt = {
          tokio::select! {
              line = stdin.next_line() => Some(EventType::Input(line.expect("can get line").expect("can read from stdin"))),
              event = swarm.next() => {
                  info!("Unhandled Swarm Event: {:?}", event);
                  None
              },
              response = response_rcv.recv() => Some(EventType::Response(response.expect("response exists")))
          }
        };

        if let Some(event) = evt {
            match event {
                EventType::Response(resp) => {
                    let json = serde_json::to_string(&resp).expect("can jsonify response");
                    swarm.floodsub.publish(TOPIC.clone(), json.as_bytes());
                },
                EventType::Input(line) => match line.as_str() {
                    "ls p" => handle_list_peers(&mut swarm).await,
                    cmd if cmd.starts_with("ls r") => handle_list_recipes(cmd, &mut swarm).await,
                    cmd if cmd.starts_with("create r") => handle_create_recipe(cmd).await,
                    cmd if cmd.starts_with("publish r") => handle_publish_recipe(cmd).await,
                    _ => error!("unknown command")
                }
            }
        }
    }
}

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

impl NetworkBehaviourEventProcess<FloodsubEvent> for RecipeBehaviour {
    fn inject_event(&mut self, event: FloodsubEvent) {
        match event {
            FloodsubEvent::Message(msg) => {
                if let Ok(resp) = serde_json::from_slice::<ListResponse>(&msg.data) {
                    if resp.receiver == PEER_ID.to_string() {
                        info!("response from {}:", msg.source);
                        resp.data.iter().for_each(|r| info!("{:?}", r));
                    }
                } else if let Ok(req) = serde_json::from_slice::<ListRequest>(&msg.data) {
                    match req.mode {
                        ListMode::ALL => {
                            info!("received ALL req: {:?} from {:?}", req, msg.source);
                            respond_with_publish_recipes(
                                self.response_sender.clone(),
                                msg.source.to_string()
                            );
                        }
                        ListMode::One(ref peer_id) => {
                            if peer_id == &PEER_ID.to_string() {
                                info!("received req: {:?} from {:?}", req, msg.source);
                                respond_with_publish_recipes(
                                    self.response_sender.clone(),
                                    msg.source.to_string()
                                );
                            }
                        }
                    }
                }
            },
            _ => ()
        }
    }
}

async fn handle_list_peers(swarm: &mut Swarm<RecipeBehaviour>) {
    info!("Discovered Peers:");
    let nodes = swarm.mdns.discovered_nodes();
    let mut unique_peers = HashSet::new();
    for peer in nodes {
        unique_peers.insert(peer);
    }
    unique_peers.iter().for_each(|p| info!("{}", p));
}

async fn handle_list_recipes(cmd: &str, swarm: &mut Swarm<RecipeBehaviour>) {
    let rest = cmd.strip_prefix("ls r ");
    match rest {
        Some("all") => {
            let req = ListRequest {
                mode: ListMode::ALL
            };
            let json = serde_json::to_string(&req).expect("can jsonify request");
            swarm.floodsub.publish(TOPIC.clone(), json.as_bytes());
        }
        Some(recipes_peer_id) => {
            let req = ListRequest {
                mode: ListMode::One(recipes_peer_id.to_owned())
            };
            let json = serde_json::to_string(&req).expect("can jsonify request");
            swarm.floodsub.publish(TOPIC.clone(), json.as_bytes());
        }
        None => {
            match read_local_recipes().await {
                Ok(v) => {
                    info!("local recipes ({})", v.len());
                    v.iter().for_each(|r| info!("{:?}", r));
                }
                Err(e) => error!("error fetching local recipes: {}", e)
            }
        }
    }
}

async fn handle_create_recipe(cmd: &str) {
    if let Some(rest) = cmd.strip_prefix("create r") {
        let elements: Vec<&str> = rest.split("|").collect();
        if elements.len() < 3 {
            info!("too few arguments - Format: name|ingredients|instructions");
        } else {
            let name = elements.get(0).expect("name is there");
            let ingredients = elements.get(1).expect("ingredients is there");
            let instruments = elements.get(2).expect("instruments is there");
            if let Err(e) = create_new_recipe(name, ingredients, instruments).await {
                error!("error creating recipe: {}", e);
            }
        }
    }
}

fn respond_with_publish_recipes(sender: mpsc::UnboundedSender<ListResponse>, receiver: String) {
    tokio::spawn(async move {
        match read_local_recipes().await {
            Ok(recipes) => {
                let resp = ListResponse {
                    mode: ListMode::ALL,
                    receiver,
                    data: recipes.into_iter().filter(|r| r.public).collect()
                };
                if let Err(e) = sender.send(resp) {
                    error!("error sending response via channel, {}", e);
                }
            }
            Err(e) => error!("error fetching local recipes to answer ALL request, {}", e)
        }
    });
}

async fn handle_publish_recipe(cmd: &str) {
    if let Some(rest) = cmd.strip_prefix("publish r") {
        match rest.trim().parse::<usize>() {
            Ok(id) => {
                if let Err(e) = publish_recipe(id).await {
                    info!("error publishing recipe with id {}, {}", id, e);
                } else {
                    info!("published recipe with id: {}", id);
                }
            }
            Err(e) => error!("invalid id: {}, {}", rest.trim(), e)
        }
    }
}

async fn create_new_recipe(name: &str, ingredients: &str, instructions: &str) -> Result<()> {
    let mut local_recipes = read_local_recipes().await?;
    let new_id = match local_recipes.iter().max_by_key(|r|r.id) {
        Some(v) => v.id + 1,
        None => 0
    };
    local_recipes.push(Recipe {
        id: new_id,
        name: name.to_owned(),
        ingredients: ingredients.to_owned(),
        instructions: instructions.to_owned(),
        public: false
    });

    write_local_recipes(&local_recipes).await?;
    info!("create recipe:");
    info!("name: {}", name);
    info!("ingredients: {}", ingredients);
    info!("instruments: {}", instructions);
    Ok(())
}

async fn publish_recipe(id: usize) -> Result<()> {
    let mut local_recipes = read_local_recipes().await?;
    local_recipes
        .iter_mut()
        .filter(|r|r.id == id)
        .for_each(|r| r.public = true);
    write_local_recipes(&local_recipes).await?;
    Ok(())
}

async fn read_local_recipes() -> Result<Recipes> {
    let content = tokio::fs::read(STORAGE_FILE_PATH).await?;
    let result = serde_json::from_slice(&content)?;
    Ok(result)
}

async fn write_local_recipes(recipes: &Recipes) -> Result<()> {
    let json = serde_json::to_string(&recipes)?;
    tokio::fs::write(STORAGE_FILE_PATH, &json).await?;
    Ok(())
}