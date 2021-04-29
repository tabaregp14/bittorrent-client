use std::env;
use std::process::exit;
use std::sync::Arc;
use crate::torrent::Torrent;
use crate::download_worker::DownloaderWorker;
use crate::client::Client;
use crate::connection::Connection;

mod connection;
mod message;
mod torrent;
mod download_worker;
mod utils;
mod client;

fn main() {
    let (torrent_path, out_path) = read_paths();

    run(torrent_path, out_path);
}

fn run(torrent_path: String, out_path: Option<String>) {
    let torrent = Torrent::open(torrent_path).unwrap();
    let client = Arc::new(Client::new(&torrent, out_path));
    let tracker = client.send_tracker_request(&torrent).unwrap();
    let mut workers = Vec::new();

    println!("{}",&torrent);
    println!("Number of peers: {}", &tracker.peers.len());

    for peer in tracker.peers {
        match Connection::new(&client, peer) {
            Ok(conn) => {
                let handler = DownloaderWorker::new(client.clone(), conn)
                    .start();

                workers.push(handler);
                // println!("Total peers connected: {}", client.workers.len());
            },
            Err(_) => {
                // println!("Could not connect to peer. Error: {}", e);
                continue;
            }
        }
    }

    for handler in workers {
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
            println!("No torrent path found.\n\
                      Usage: bittorrent-client <torrent file path> [out path]");
            exit(0);
        }
    }
}
