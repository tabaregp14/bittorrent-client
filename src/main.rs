use std::env;
use std::path::Path;
use rand::Rng;
use crate::torrent::Torrent;
use crate::connection::Connection;
use std::net::{SocketAddr, IpAddr};

mod connection;
mod message;
mod torrent;
mod tracker_handler;

fn main() {
    let args = env::args().collect::<Vec<String>>();
    let path = Path::new(&args[1]);
    let torrent = Torrent::open(path).unwrap();
    let peer_id = rand::thread_rng().gen::<[u8; 20]>().to_vec();
    let port = 6881;
    let mut res = tracker_handler::request_peers(&torrent, &peer_id, &port).unwrap();

    let test_peer = res.peers.pop().unwrap();

    let peer_addr = SocketAddr::new(
        IpAddr::from(test_peer.ip.to_owned()),
        test_peer.port.to_owned()
    );
    let conn = Connection::connect(test_peer, torrent.info_hash, peer_id);

    match conn {
        Ok(mut conn) => {
            let hs = conn.complete_handshake().unwrap();
            let msg = message::Message::read(&conn.stream).unwrap();

            println!("{:?}", msg);
        },
        Err(_) => println!("Connection with {} timed out.", peer_addr)
    }

//    let hs = conn.complete_handshake().unwrap();
//    let msg = message::Message::read(&conn.stream).unwrap();
//
//    println!("{:?}", msg);
}
