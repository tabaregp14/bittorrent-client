use std::error::Error;
use std::{fs, fmt, io};
use std::path::Path;
use serde::{Deserialize, Serialize};
use serde_bencode;
use serde_bytes::ByteBuf;
use sha1::{Digest, Sha1};
use crate::message::Message;
use crate::connection::Connection;

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
    requested: u32,
    downloaded: u32,
    buf: Vec<u8>,
    concurrent_requests: u8
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

    fn new(index: u32, length: u32) -> DownloadPieceState {
        DownloadPieceState {
            index,
            requested: 0,
            downloaded: 0,
            buf: vec![0; length as usize],
            concurrent_requests: 0
        }
    }

    fn send_request(&mut self, block_size: u32, conn: &mut Connection) -> Result<(), io::Error> {
        conn.send(Message::Request(self.index, self.requested, block_size))?;

        self.requested += block_size;
        self.concurrent_requests += 1;

        Ok(())
    }

    fn read_message(&mut self, conn: &mut Connection) -> Result<(), io::Error> {
        match conn.read()? {
            Message::Piece(index, begin, block) => {
                let length = (&block.len() + 0) as u32;

                if index != self.index {
                    println!("Expected piece ID {} but got {}", &self.index, &index);

                    self.concurrent_requests -= 1;
                    return Ok(());
                }

                self.buf.splice(begin as usize..begin as usize + block.len(), block);
                self.downloaded += length as u32;
                self.concurrent_requests -= 1;

                Ok(())
            },
            Message::Have(index) => {
                println!("Have: {}", &index);
                conn.set_piece(&index);
                self.concurrent_requests -= 1;

                Ok(())
            },
            Message::Choke => {
                println!("Choked");
                conn.chocked = true;
                self.concurrent_requests -= 1;

                Ok(())
            },
            _ => {
                println!("Other message");

                Ok(())
            }
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

#[cfg(test)]
mod tests {

}