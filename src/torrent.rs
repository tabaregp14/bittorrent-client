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


#[cfg(test)]
mod tests {

}