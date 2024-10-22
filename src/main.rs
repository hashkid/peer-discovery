use std::{env::args, num::NonZeroUsize, str::FromStr, time::Duration};

use libp2p::{futures::StreamExt, identify, identity::Keypair, kad::{store::MemoryStore, GetRecordOk, Record, RecordKey}, noise, swarm::{NetworkBehaviour, SwarmEvent}, tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder};
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

    let mut swarm = if args.len() >= 2 {
            SwarmBuilder::with_new_identity()
        } else {
            let keypair = Keypair::ed25519_from_bytes([1; 32]).expect("error key");
            SwarmBuilder::with_existing_identity(keypair)
        }.with_tokio()
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

    let port = if args.len() > 1 { 0 } else {3232};
    let addr = format!("/ip4/127.0.0.1/tcp/{port}").parse().expect("invalid addr");
    swarm.listen_on(addr).expect("failed to listen on all interfaces");

    if let Some(boot) = args.get(1) {
        dail_bootstrap_nodes(&mut swarm, boot);
    }
    let mut interval = tokio::time::interval(Duration::from_secs(10));

    // let _ = swarm.behaviour_mut().kad.put_record(Record::new([1].to_vec(), vec![0]), libp2p::kad::Quorum::All);
    if args.len() < 2 {
        swarm.behaviour_mut().kad.set_mode(Some(libp2p::kad::Mode::Server));
    }

    let to_search = PeerId::random();

    loop {
        select! {
            swarm_event = swarm.select_next_some() => match swarm_event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Local node is listening on {address}/p2p/{}", swarm.local_peer_id());
                },
                SwarmEvent::Behaviour(DiscoveryBehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. })) => {
                    info.listen_addrs.iter().for_each(|addr| {
                        println!("Discovered new address: {addr}/p2p/{peer_id} ");
                        swarm.behaviour_mut().kad.add_address(&peer_id, addr.clone());
                    });
                    println!("remote {:?}", info.observed_addr);
                    let _ = swarm.behaviour_mut().kad.bootstrap();
                },
                SwarmEvent::Behaviour(DiscoveryBehaviourEvent::Kad(libp2p::kad::Event::RoutingUpdated { 
                    peer, is_new_peer, addresses, ..
                })) => {
                    println!("KAD added {:?}", peer);
                    if is_new_peer {
                        let addr = addresses.first().clone();
                        swarm.behaviour_mut().kad.add_address(&peer, addr);
                    }
                },
                SwarmEvent::Behaviour(DiscoveryBehaviourEvent::Kad(libp2p::kad::Event::OutboundQueryProgressed { 
                    result,
                    ..
                 })) => {
                    match result {
                        libp2p::kad::QueryResult::GetClosestPeers(qr) => {
                            match qr {
                                Ok(peer) => {
                                    if peer.peers.len() > 0 {
                                        println!("Kad events: {:?}", peer.peers);
                                    }
                                },
                                Err(e) => {
                                    println!("Error {}", e)
                                }
                            }
                        }
                        _ => {
                            println!("events: {:?}", result)
                        }
                    }
                    
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

                // put(&mut swarm);
                println!("Connected peers: {:?}", swarm.connected_peers().map(|p| p.clone()).collect::<Vec<_>>());
                // swarm.behaviour_mut().kad.
                
                // discover(&mut swarm, to_search);
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

fn discover(swarm: &mut Swarm<DiscoveryBehaviour>, to_search: PeerId) {

    // let to_search: PeerId = PeerId::random();
    println!("Searching for the closest peers to {:?}", to_search);
    swarm.behaviour_mut().kad.get_closest_peers(to_search);
    let _ = swarm.behaviour_mut().kad.bootstrap();
}

fn put(swarm: &mut Swarm<DiscoveryBehaviour>) {

    let pk_record = libp2p::kad::Record::new(swarm.local_peer_id().to_bytes(), [1].to_vec());

    swarm
    .behaviour_mut()
    .kad.put_record(pk_record, libp2p::kad::Quorum::N(NonZeroUsize::new(2).unwrap()))
    .expect("failed to pub");
}