use std::error::Error;
use std::{fs, fmt, io, thread};
use std::path::Path;
use serde::{Deserialize, Serialize};
use serde_bencode;
use serde_bytes::ByteBuf;
use sha1::{Digest, Sha1};
use crate::message::Message;
use crate::connection::Connection;
use std::sync::Mutex;
use std::fs::File;
use std::io::{Write, Seek, SeekFrom};

type PieceHash = Vec<u8>;
type DownloadedTorrent = Vec<u8>;

#[derive(Deserialize, Serialize)]
struct TorrentInfo {
    name: String,
    files: Option<Vec<TorrentSubFile>>,
    length: Option<u64>,
    #[serde(rename = "piece length")]
    piece_length: u32,
    pieces: ByteBuf
}

#[derive(Deserialize)]
struct BencodeTorrent {
    announce: String,
    info: TorrentInfo
}

#[derive(Deserialize, Serialize)]
pub struct TorrentSubFile {
    pub path: Vec<String>,
    length: u64
}

#[derive(Deserialize)]
pub struct Torrent {
    pub name: String,
    pub announce: String,
    pub info_hash: Vec<u8>,
    pub files: Option<Vec<TorrentSubFile>>,
    pub pieces: Vec<PieceHash>,
    pub piece_length: u32,
    length: Option<u64> // file size
}

pub struct DownloadTorrentState {
    // pub peer_id: Vec<u8>,
    piece_queue: Mutex<Vec<Piece>>,
    done_pieces: Mutex<u32>,
    length: u32,
    file: Mutex<File>
}

#[derive(Clone)]
struct Piece {
    index: u32,
    hash: PieceHash,
    length: u32, // piece size
    begin: u32,
    end: u32
}

struct DownloadPieceState {
    index: u32,
    begin: u32,
    requested_blocks: Vec<Block>,
    blocks_done: u8,
    block_queue: Vec<Block>,
    buf: Vec<u8>
}

struct Block {
    index: u32,
    begin: u32,
    end: u32,
    length: u32,
    data: Option<Vec<u8>>
}

impl BencodeTorrent {
    fn to_torrent(self) -> Result<Torrent, serde_bencode::Error> {
        let info_bytes = serde_bencode::to_bytes(&self.info)?;

        Ok(Torrent {
            info_hash: hash_sha1(&info_bytes),
            name: self.info.name,
            announce: self.announce,
            files: self.info.files,
            length: self.info.length,
            piece_length: self.info.piece_length,
            pieces: self.info.pieces.chunks(20)
                .map(|s| s.to_vec())
                .collect()
        })
    }
}

impl Torrent {
    pub fn open(path: &Path) -> Result<Torrent, Box<dyn Error>> {
        let file = fs::read(path)?;
        let bencode_torrent = serde_bencode::from_bytes::<BencodeTorrent>(&file)?;
        let torrent = bencode_torrent.to_torrent()?;

        Ok(torrent)
    }

    fn create_pieces_queue(&self) -> Vec<Piece> {
        let mut work_queue = Vec::<Piece>::new();
        let piece_length = self.piece_length as u64;
        let mut length = piece_length;

        for (index, hash) in self.pieces.iter().enumerate() {
            if index == self.pieces.len() - 1 && self.calculate_length() % piece_length > 0 {
                length = self.calculate_length() % piece_length;
            }
            let piece = Piece::new(index as u32,
                                   hash.to_owned(),
                                   length as u32);

            work_queue.push(piece);
        }

        work_queue
    }

    pub fn download(&self, conn: &mut Connection) -> DownloadedTorrent {
        let mut buf = vec![0; self.calculate_length() as usize];
        let mut work_queue = self.create_pieces_queue();
        let mut done_pieces = 0;

        while done_pieces < self.pieces.len() {
            let work_piece = work_queue.pop().unwrap();

            if !conn.has_piece(&work_piece.index) {
                println!("Peer doesn't have piece {}", &work_piece.index);

                work_queue.push(work_piece);

                continue;
            }

            println!("DOWNLOADING PIECE: {}", &work_piece.index);
            let piece_result = work_piece.try_download(conn);

            match piece_result {
                Ok(piece) => {
                    buf.splice(work_piece.begin as usize..work_piece.end as usize, piece);
                    done_pieces += 1;

                    println!("Done pieces: {} / {}", &done_pieces, &self.pieces.len());
                }
                Err(e) => {
                    println!("ERROR: {}", e);

                    work_queue.push(work_piece.to_owned());
                }
            }
        }

        buf
    }

    pub fn calculate_length(&self) -> u64 {
        match self.length {
            Some(length) => length,
            None => self.files.as_ref()
                .unwrap()
                .iter()
                .fold(0, |acc, file| acc + file.length)
        }
    }
}

impl DownloadTorrentState {
    pub const MAX_CONCURRENT_PEERS: u8 = 20;

    pub fn new(torrent: &Torrent) -> DownloadTorrentState {
        let file = File::create(&torrent.name).unwrap();

        file.set_len(torrent.calculate_length()).unwrap();

        DownloadTorrentState {
            done_pieces: Mutex::new(0),
            piece_queue: Mutex::new(torrent.create_piece_queue()),
            length: torrent.pieces.len() as u32,
            file: Mutex::new(file)
        }
    }

    pub fn is_done(&self) -> bool {
        let done_pieces = self.done_pieces.lock().unwrap();

        if *done_pieces >= self.length {
            return true;
        }

        false
    }

    fn get_piece_from_queue(&self) -> Option<Piece> {
        let mut piece_queue = self.piece_queue.lock().unwrap();

        piece_queue.pop()
    }
}

impl Piece {
    const MAX_BLOCK_SIZE: u32 = 16384;

    pub fn new(index: u32, hash: PieceHash, length: u32) -> Self {
        let begin = index * length;
        let end = begin + length;

        Piece {
            index,
            hash,
            length,
            begin,
            end
        }
    }

    fn try_download(&self, conn: &mut Connection) -> Result<Vec<u8>, Box<dyn Error + '_>> {
        let mut state = DownloadPieceState::new(self.index, self.length);
        let mut block_size = Self::MAX_BLOCK_SIZE;

        while state.downloaded < self.length {
            if !conn.chocked {
                while state.concurrent_requests < DownloadPieceState::MAX_CONCURRENT_REQUESTS && state.requested < self.length {
                    if self.length - state.requested < Self::MAX_BLOCK_SIZE {
                        block_size = self.length - state.requested;
                    }

                    state.send_request(block_size, conn)?;
                }
            }

            state.read_message(conn)?;
        }

        self.check_integrity(hash_sha1(&state.buf))?;
        println!("Piece {} finished", &self.index);

        Ok(state.buf)
    }

    fn check_integrity(&self, hash: PieceHash) -> Result<(), IntegrityError> {
        if self.hash.eq(&hash) {
            println!("Correct hash");

            Ok(())
        } else {
            Err(IntegrityError(&self.hash, hash))
        }
    }
}

impl DownloadPieceState {
    const MAX_CONCURRENT_REQUESTS: u8 = 5;

    fn new(piece: &Piece) -> DownloadPieceState {
        DownloadPieceState {
            index: piece.index,
            begin: piece.begin,
            requested_blocks: Vec::new(),
            buf: vec![0; piece.length as usize],
            block_queue: piece.create_block_queue(),
            blocks_done: 0
        }
    }

    fn send_request(&mut self, block: Block, conn: &mut Connection) -> Result<(), io::Error> {
        conn.send(Message::Request(self.index, block.begin, block.length))?;
        self.requested_blocks.push(block);

        Ok(())
    }

    fn read_message(&mut self, conn: &mut Connection) -> Option<Block> {
        match conn.read().ok()? {
            Message::Piece(index, begin, block_data) => {
                if index != self.index {
                    println!("Thread [{:?}]: Expected piece ID {} but got {}", thread::current().name().unwrap(), &self.index, &index);

                    return None;
                }

                let block_index = self.requested_blocks.iter().position(|b| b.begin == begin);

                match block_index {
                    Some(block_index) => {
                        let mut block = self.requested_blocks.remove(block_index);

                        self.blocks_done += 1;
                        block.data = Some(block_data);

                        Some(block)
                    }
                    None => {
                        println!("Thread [{:?}]: Received block was not requested", thread::current().name().unwrap());

                        None
                    }
                }
            },
            Message::Have(index) => {
                println!("Have: {}", &index);
                conn.set_piece(&index);

                None
            },
            Message::Choke => {
                println!("Thread [{:?}]: Choked", thread::current().name().unwrap());
                conn.chocked = true;

                None
            },
            Message::Unchoke => {
                println!("Thread [{:?}]: Unchoked", thread::current().name().unwrap());
                conn.chocked = false;

                None
            },
            _ => {
                println!("Thread [{:?}]: Other message", thread::current().name().unwrap());

                None
            }
        }
    }

    // TODO: handle Option
    fn store_in_buffer(&mut self, block: Block) {
        self.buf.splice(block.begin as usize..block.end as usize, block.data.unwrap());
    }

    fn copy_to_file(&self, file: &mut File) -> Result<(), io::Error> {
        file.seek(SeekFrom::Start(self.begin as u64))?;
        file.write_all(&self.buf)?;

        Ok(())
    }
}

impl Block {
    fn new(index: u32, begin: u32, end: u32, length: u32) -> Block {
        Block {
            index,
            begin,
            end,
            length,
            data: None
        }
    }
}

fn hash_sha1(v: &Vec<u8>) -> Vec<u8> {
    let mut hasher = Sha1::new();

    hasher.input(v);

    hasher.result().to_vec()
}

impl fmt::Display for Torrent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut files_names = Vec::new();

        match &self.files {
            Some(files) => {
                files_names = files.into_iter()
                    .map(|f| &f.path[&f.path.len() - 1])
                    .collect::<Vec<&String>>();
            },
            None => files_names.push(&self.name)
        }

        write!(f, "----Name: {}\n----Files: {:?}\n----Size: {}\n----Number of pieces: {}\n----Size of pieces: {}",
               self.name,
               files_names,
               self.calculate_length(),
               self.pieces.len(),
               self.piece_length
        )
    }
}

#[derive(Debug)]
struct IntegrityError<'a>(&'a PieceHash, PieceHash);

impl<'a> fmt::Display for IntegrityError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Incorrect piece hash, Expected hash: {:?} but got {:?}", self.0, self.1)
    }
}
impl<'a> Error for IntegrityError<'a> {}

#[derive(Debug)]
enum DownloadPieceError<'a> {
    WrongHash(IntegrityError<'a>),
    IOError(io::Error)
}

impl<'a> fmt::Display for DownloadPieceError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DownloadPieceError::WrongHash(e) =>
                write!(f, "{}", e),
            DownloadPieceError::IOError(..) =>
                write!(f, "Error sending message")
        }
    }
}
impl<'a> From<IntegrityError<'a>> for DownloadPieceError<'a> {
    fn from(err: IntegrityError) -> DownloadPieceError {
        DownloadPieceError::WrongHash(err)
    }
}
impl<'a> From<io::Error> for DownloadPieceError<'a> {
    fn from(err: io::Error) -> DownloadPieceError<'a> {
        DownloadPieceError::IOError(err)
    }
}
