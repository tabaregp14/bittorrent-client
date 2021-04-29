use std::time::Duration;
use std::sync::{Mutex, MutexGuard};
use std::fs::File;
use std::path::Path;
use std::{io, fmt};
use std::env::set_current_dir;
use std::collections::VecDeque;
use rand::Rng;
use reqwest::Url;
use crate::connection::TrackerResponse;
use crate::torrent::{Torrent, Piece};
use crate::utils::url_encode;

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
    pub total_pieces: u32,
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

    pub fn get_done_pieces(&self) -> MutexGuard<u32> {
        self.torrent.done_pieces
            .lock()
            .unwrap()
    }

    pub fn get_file(&self) -> MutexGuard<File> {
        self.file.lock().unwrap()
    }

    pub fn send_tracker_request(&self, torrent: &Torrent) -> Result<TrackerResponse, TrackerError> {
        let mut buf = Vec::new();
        let url = self.parse_url(&torrent);
        let req_client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()?;
        let mut res = req_client.get(url)
            .send()?;

        res.copy_to(&mut buf)?;

        let tracker_response = serde_bencode::from_bytes::<TrackerResponse>(&buf.as_slice())?;

        Ok(tracker_response)
    }

    fn parse_url(&self, torrent: &Torrent) -> Url {
        let url_hash = url_encode(&self.torrent.info_hash);
        let url_peer_id = url_encode(&self.id);
        let base_url = format!("{}?info_hash={}&peer_id={}", torrent.announce, url_hash, url_peer_id);
        let url_params = [
            ("port", self.port.to_string()),
            ("uploaded", self.uploaded.to_string()),
            ("downloaded", self.downloaded.to_string()),
            ("compact", "1".to_string()),
            ("left", torrent.calculate_length().to_string())
        ];
        let url = Url::parse_with_params(base_url.as_str(),&url_params).unwrap();

        url
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
            total_pieces: torrent.pieces.len() as u32,
            info_hash: torrent.info_hash.to_owned(),
        }
    }

    pub fn is_done(&self) -> bool {
        let done_pieces = self.done_pieces.lock().unwrap();

        if *done_pieces >= self.total_pieces {
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

#[derive(Debug)]
pub enum TrackerError {
    SerializationError(serde_bencode::Error),
    RequestError(reqwest::Error)
}

impl fmt::Display for TrackerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::SerializationError(e) =>
                write!(f, "{}", e),
            Self::RequestError(e) =>
                write!(f, "{}", e)
        }
    }
}
impl From<serde_bencode::Error> for TrackerError {
    fn from(err: serde_bencode::Error) -> Self {
        Self::SerializationError(err)
    }
}
impl From<reqwest::Error> for TrackerError {
    fn from(err: reqwest::Error) -> Self {
        Self::RequestError(err)
    }
}
