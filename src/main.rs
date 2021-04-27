use std::env;
use std::path::Path;
use std::process::exit;
use std::sync::Arc;
use rand::Rng;
use crate::torrent::Torrent;
use crate::download_worker::{DownloaderWorker, TorrentState};
use crate::connection::Connection;

mod connection;
mod message;
mod torrent;
mod tracker_handler;
mod download_worker;
mod utils;

fn main() {
    let (torrent_path, out_path) = read_paths();

    run(torrent_path, out_path);
}

fn run(torrent_path: String, out_path: Option<String>) {
    const PORT: u16 = 6881;

    let peer_id = rand::thread_rng().gen::<[u8; 20]>().to_vec();
    let t_path = Path::new(&torrent_path);
    let torrent = Torrent::open(t_path).unwrap();
    let torrent_state = Arc::new(TorrentState::new(&torrent));
    let res = tracker_handler::request_peers(&torrent, &peer_id, &PORT).unwrap();
    let mut peer_queue = res.peers;
    let mut workers = Vec::new();

    println!("Torrent:\n{}",&torrent);
    println!("Number of peers: {}", &peer_queue.len());

    while workers.len() < TorrentState::MAX_CONCURRENT_PEERS && peer_queue.len() > 0 {
        let peer = peer_queue.pop().unwrap();
        let torrent_state = Arc::clone(&torrent_state);

        match Connection::connect(peer, &torrent.info_hash, &peer_id) {
            Ok(conn) => {
                let handler = DownloaderWorker::new(torrent_state, conn).start();

                workers.push(handler);
                // println!("Total peers connected: {}", workers.len());
            },
            Err(_) => {
                // println!("Could not connect to peer. Error: {}", e);
                continue;
            }
        }
    }

    for handler in workers {
        match handler.join() {
            Ok(_) => {},
            Err(e) => println!("Error joining worker with main thread: {:?}", e)
        }
    }
}

fn read_paths() -> (String, Option<String>) {
    let args = env::args().collect::<Vec<String>>();
    let torrent_path = args.get(1);
    let out_path = args.get(2).cloned();

    match torrent_path {
        Some(path) => (path.to_owned(), out_path),
        None => {
            println!("Use bittorrent-client <torrent file> [out path]");
            exit(0);
        }
    }
}
