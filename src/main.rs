use std::env;
use std::process::exit;
use std::sync::Arc;
use crate::torrent::Torrent;
use crate::download_worker::{DownloaderWorker, TorrentState};
use crate::tracker_handler::Tracker;
use crate::client::Client;

mod connection;
mod message;
mod torrent;
mod tracker_handler;
mod download_worker;
mod utils;
mod client;

fn main() {
    let (torrent_path, out_path) = read_paths();

    run(torrent_path, out_path);
}

fn run(torrent_path: String, out_path: Option<String>) {
    let torrent = Torrent::open(torrent_path).unwrap();
    let torrent_state = Arc::new(TorrentState::new(&torrent, out_path));
    let mut client = Client::new(&torrent.info_hash);
    let mut tracker = Tracker::send_request(&torrent, &client).unwrap();

    println!("{}",&torrent);
    println!("Number of peers: {}", &tracker.peers.len());

    while client.workers.len() < TorrentState::MAX_CONCURRENT_PEERS && tracker.peers.len() > 0 {
        let peer = tracker.peers.pop().unwrap();

        match client.connect(peer) {
            Ok(conn) => {
                let torrent_state = Arc::clone(&torrent_state);
                let handler = DownloaderWorker::new(torrent_state, conn).start();

                client.workers.push(handler);
                // println!("Total peers connected: {}", client.workers.len());
            },
            Err(_) => {
                // println!("Could not connect to peer. Error: {}", e);
                continue;
            }
        }
    }

    for handler in client.workers {
        handler.join().expect("Error joining worker with main thread.");
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
