use std::io;
use std::path::Path;
use std::error::Error;
use rand::Rng;
use crate::torrent_handler::Torrent;
use crate::connection_handler::Connection;

mod connection_handler;
mod message;
mod torrent_handler;
mod tracker_handler;

fn main() {
    let input = read_input().unwrap();
    let path = Path::new(&input);
    let torrent = Torrent::open(path).unwrap();
    let peer_id = rand::thread_rng().gen::<[u8; 20]>().to_vec();
    let port = 6881;
    let mut res = tracker_handler::request_peers(&torrent, &peer_id, &port).unwrap();

    let test_peer = res.peers.pop().unwrap();

    let mut conn = Connection::connect(test_peer, torrent.info_hash, peer_id).unwrap();
    let hs = conn.complete_handshake().unwrap();
    let msg = message::Message::read(&conn.stream).unwrap();

    println!("{:?}", msg);
}

fn read_input() -> Result<String, Box<dyn Error>> {
    let mut input = String::new();

    println!("Path of the torrent");
    io::stdin().read_line(&mut input)?;

    input = input.trim().parse()?;

    Ok(input)
}
