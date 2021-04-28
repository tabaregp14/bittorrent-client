use std::net::{TcpStream, SocketAddr};
use std::time::Duration;
use std::sync::{Mutex, MutexGuard};
use std::fs::File;
use std::path::Path;
use std::io;
use std::env::set_current_dir;
use std::collections::VecDeque;
use rand::Rng;
use crate::connection::{Connection, ConnectionError};
use crate::tracker_handler::Peer;
use crate::torrent::{Torrent, Piece};

pub struct Client {
    pub id: Vec<u8>,
    pub port: u16,
    pub uploaded: u32,
    pub downloaded: u32,
    pub torrent: TorrentState,
    file: Mutex<File>,
}

pub struct TorrentState {
    pub info_hash: Vec<u8>,
    pub length: u32,
    piece_queue: Mutex<VecDeque<Piece>>,
    done_pieces: Mutex<u32>,
}

impl Client {
    const PORT: u16 = 6881;

    pub fn new<P: AsRef<Path>>(torrent: &Torrent, out_path: Option<P>) -> Client {
        let file = Self::create_files(torrent, out_path).unwrap();

        Client {
            id: Self::generate_random_id(),
            port: Self::PORT,
            uploaded: 0,
            downloaded: 0,
            file: Mutex::new(file),
            torrent: TorrentState::new(torrent)
        }
    }

    pub fn connect(&self, peer: Peer) -> Result<Connection, ConnectionError> {
        let addr = SocketAddr::from(peer);
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;

        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;

        let mut conn = Connection::new(stream, peer);

        conn.complete_handshake(self)?;

        Ok(conn)
    }

    pub fn get_done_pieces(&self) -> MutexGuard<u32> {
        self.torrent.done_pieces
            .lock()
            .unwrap()
    }

    pub fn get_file(&self) -> MutexGuard<File> {
        self.file.lock().unwrap()
    }

    fn generate_random_id() -> Vec<u8> {
        let id = rand::thread_rng().gen::<[u8; 20]>().to_vec();

        id
    }

    // TODO: add multiple files creation
    fn create_files<P: AsRef<Path>>(torrent: &Torrent, path: Option<P>) -> io::Result<File> {
        match path {
            Some(path) => set_current_dir(path)?,
            None => {}
        }

        let file = File::create(&torrent.name)?;

        file.set_len(torrent.calculate_length())?;

        Ok(file)
    }
}

impl TorrentState {
    fn new(torrent: &Torrent) -> TorrentState {
        TorrentState {
            done_pieces: Mutex::new(0),
            piece_queue: Mutex::new(torrent.create_piece_queue()),
            length: torrent.pieces.len() as u32,
            info_hash: torrent.info_hash.to_owned(),
        }
    }

    pub fn is_done(&self) -> bool {
        let done_pieces = self.done_pieces.lock().unwrap();

        if *done_pieces >= self.length {
            return true;
        }

        false
    }

    pub fn get_piece_from_queue(&self) -> Option<Piece> {
        let mut piece_queue = self.piece_queue
            .lock()
            .unwrap();

        piece_queue.pop_front()
    }

    pub fn push_piece_to_queue(&self, piece: Piece) {
        let mut pieces_queue = self.piece_queue
            .lock()
            .unwrap();

        pieces_queue.push_back(piece);
    }
}