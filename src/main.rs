use std::{str::FromStr, time::Duration};

use libp2p::{futures::StreamExt, identify, kad::{store::MemoryStore, Record, RecordKey}, noise, swarm::{NetworkBehaviour, SwarmEvent}, tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder};
use tokio::select;
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

    let mut swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        // .with_quic()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        ).expect("build tcp error")
        .with_behaviour(|kp| {

            DiscoveryBehaviour {
                kad: libp2p::kad::Behaviour::new(kp.public().to_peer_id(), MemoryStore::new(kp.public().to_peer_id())),
                identify: identify::Behaviour::new(
                    identify::Config::new("/testnet/0.1.0".into(), kp.public().clone())
                ),
            }
        }).expect("failed build swarm")
        .with_swarm_config(|c| {c.with_idle_connection_timeout(Duration::from_secs(300))})
        .build();

    let addr = "/ip4/127.0.0.1/tcp/0".parse().expect("invalid addr");
    swarm.listen_on(addr).expect("failed to listen on all interfaces");

    if let Some(boot) = args.get(1) {
        dail_bootstrap_nodes(&mut swarm, boot);
    }
    let mut interval = tokio::time::interval(Duration::from_secs(10));

    let _ = swarm.behaviour_mut().kad.put_record(Record::new([1].to_vec(), vec![0]), libp2p::kad::Quorum::All);

    loop {
        select! {
            swarm_event = swarm.select_next_some() => match swarm_event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Local node is listening on {address}/p2p/{}", swarm.local_peer_id());
                },
                SwarmEvent::Behaviour(DiscoveryBehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. })) => {
                    info.listen_addrs.iter().for_each(|addr| {
                        println!("Discovered new address: {peer_id} {addr}");
                        swarm.behaviour_mut().kad.add_address(&peer_id, addr.clone());
                    });
                    let _ = swarm.behaviour_mut().kad.bootstrap();
                },
                SwarmEvent::Behaviour(DiscoveryBehaviourEvent::Kad(e)) => {
                    println!("Kad events: {:?}", e);
                },
                SwarmEvent::ConnectionEstablished { peer_id, num_established, endpoint, ..} => {
                    let connected = swarm.connected_peers().map(|p| p.clone()).collect::<Vec<_>>();
                    if connected.len() > 0 {
                        swarm.behaviour_mut().identify.push(connected);
                    }
                    println!("Connected to {peer_id}, Swarm Connection Established, {num_established} {:?} ", endpoint);                  
                },
                SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                    println!("Connection {peer_id} closed.{:?}", cause);
                },
                _ => {
                    // debug!("Swarm event: {:?}", swarm_event);
                },
            },
            _ = interval.tick() => {
                println!("Connected peers: {:?}", swarm.connected_peers().map(|p| p.clone()).collect::<Vec<_>>());
                // let peer_id = swarm.local_peer_id().clone();
                let q = swarm.behaviour_mut().kad.get_closest_peers([1].to_vec());
                println!("Query Id: {}", q);
            }
        }
    }

}

fn dail_bootstrap_nodes(swarm: &mut Swarm<DiscoveryBehaviour>, bootstrap_nodes: &String) {
    
    let addr_text = bootstrap_nodes;
    let address = Multiaddr::from_str(addr_text).expect("invalid bootstrap node address");
    let peer = PeerId::from_str(addr_text.split("/").last().unwrap()).expect("invalid peer id");
    swarm.behaviour_mut().kad.add_address(&peer, address);
    println!("Adding bootstrap node: {:?}", addr_text);

    if bootstrap_nodes.len() > 0 {
        match swarm.behaviour_mut().kad.bootstrap() {
            Ok(_) => {
                println!("KAD bootstrap successful");
            }
            Err(e) => {
                println!("Failed to start KAD bootstrap: {:?}", e);
            }
        }
    }
}