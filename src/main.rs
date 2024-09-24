use std::str::FromStr;

use libp2p::{futures::StreamExt, identify, kad::store::MemoryStore, swarm::{NetworkBehaviour, SwarmEvent}, Multiaddr, PeerId, Swarm, SwarmBuilder};
use tokio::select;
use tracing::info;
use std::env;

#[derive(NetworkBehaviour)]
pub struct DiscoveryBehaviour {
    pub kad: libp2p::kad::Behaviour<MemoryStore>,
    pub identify: libp2p::identify::Behaviour,
}

#[tokio::main(flavor = "multi_thread", worker_threads=4)]
async fn main() {
    let args: Vec<String> = env::args().collect();
    println!("start {:?}", args);

    let mut swarm = SwarmBuilder::with_new_identity().with_tokio().with_quic().with_behaviour(|kp| {

        DiscoveryBehaviour {
            kad: libp2p::kad::Behaviour::new(kp.public().to_peer_id(), MemoryStore::new(kp.public().to_peer_id())),
            identify: identify::Behaviour::new(
                identify::Config::new("/testnet/0.1.0".into(), kp.public().clone())
            ),
        }
    }).expect("failed build swarm").build();

    let addr = "/ip4/0.0.0.0/tcp/0".parse().expect("invalid addr");
    swarm.listen_on(addr).expect("failed to listen on all interfaces");

    loop {
        select! {
            swarm_event = swarm.select_next_some() => match swarm_event {
                SwarmEvent::Behaviour(DiscoveryBehaviourEvent::Identify(a)) => {
                    info!("behave events: {:?}", a);
                },
                SwarmEvent::Behaviour(DiscoveryBehaviourEvent::Kad(e)) => {
                    info!("behave events: {:?}", e);
                },
                SwarmEvent::NewListenAddr { address, .. } => {
                    info!("Local node is listening on {address}");
                },
                SwarmEvent::ConnectionEstablished { peer_id, num_established, endpoint, ..} => {
                    let connected = swarm.connected_peers().map(|p| p.clone()).collect::<Vec<_>>();
                    if connected.len() > 0 {
                        swarm.behaviour_mut().identify.push(connected);
                    }
                    info!("Connected to {peer_id}, Swarm Connection Established, {num_established} {:?} ", endpoint);                  
                },
                SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                    info!("Connection {peer_id} closed.{:?}", cause);
                },
                _ => {
                    // debug!("Swarm event: {:?}", swarm_event);
                },
            },
        }
    }

}

fn dail_bootstrap_nodes(swarm: &mut Swarm<DiscoveryBehaviour>, bootstrap_nodes: &String) {
    
    let addr_text = bootstrap_nodes;
    let address = Multiaddr::from_str(addr_text).expect("invalid bootstrap node address");
    let peer = PeerId::from_str(addr_text.split("/").last().unwrap()).expect("invalid peer id");
    swarm.behaviour_mut().kad.add_address(&peer, address);
    info!("Adding bootstrap node: {:?}", addr_text);

    if bootstrap_nodes.len() > 0 {
        match swarm.behaviour_mut().kad.bootstrap() {
            Ok(_) => {
                info!("KAD bootstrap successful");
            }
            Err(e) => {
                info!("Failed to start KAD bootstrap: {:?}", e);
            }
        }
    }
}