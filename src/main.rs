use std::io;
use std::path::Path;
use std::error::Error;
use rand::Rng;
use rand::distributions::Alphanumeric;
use crate::torrent_handler::Torrent;

mod torrent_handler;
mod tracker_handler;

fn main() {
    let input = read_input().unwrap();
    let path = Path::new(&input);
    let torrent = Torrent::open(path).unwrap();
    let peer_id = rand::thread_rng().sample_iter(Alphanumeric).take(20).collect();
    let port = 6881;
    let res = tracker_handler::request_peers(torrent, &peer_id, &port).unwrap();

    println!("{:#?}", res);
}

fn read_input() -> Result<String, Box<dyn Error>> {
    let mut input = String::new();

    println!("Path of the torrent");
    io::stdin().read_line(&mut input)?;

    input = input.trim().parse()?;

    Ok(input)
}
