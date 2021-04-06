use std::collections::VecDeque;
use std::error::Error;
use std::{fs, fmt};
use std::path::Path;
use serde::{Deserialize, Serialize};
use serde_bencode;
use serde_bytes::ByteBuf;
use sha1::{Digest, Sha1};

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
struct TorrentSubFile {
    path: Vec<String>,
    length: u64
}

#[derive(Deserialize)]
pub struct Torrent {
    pub announce: String,
    pub info_hash: Vec<u8>,
    pub name: String,
    pub pieces: Vec<PieceHash>,
    files: Option<Vec<TorrentSubFile>>,
    piece_length: u32,
    length: Option<u64> // file size
}

#[derive(Clone)]
pub struct Piece {
    pub index: u32,
    pub length: u32, // piece size
    pub begin: u32,
    hash: PieceHash,
    end: u32
}

pub struct Block {
    // index: u32,
    pub begin: u32,
    pub end: u32,
    pub length: u32,
    pub data: Option<Vec<u8>>
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

    pub fn create_piece_queue(&self) -> VecDeque<Piece> {
        let mut piece_queue = VecDeque::<Piece>::new();
        let piece_length = self.piece_length as u64;
        let mut length = piece_length;

        for (index, hash) in self.pieces.iter().enumerate() {
            if index == self.pieces.len() - 1 && self.calculate_length() % piece_length > 0 {
                length = self.calculate_length() % piece_length;
            }

            let piece = Piece::new(index as u32,
                                   (index * piece_length as usize) as u32,
                                   length as u32,
                                   hash.to_owned());

            piece_queue.push_back(piece);
        }

        piece_queue
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

    fn new(index: u32, begin: u32, length: u32, hash: PieceHash) -> Self {
        let end = begin + length;

        Piece {
            index,
            hash,
            length,
            begin,
            end
        }
    }

    pub fn create_block_queue(&self) -> Vec<Block> {
        let mut block_queue = Vec::<Block>::new();
        let mut block_length = Self::MAX_BLOCK_SIZE;
        let num_of_blocks = (self.length as f32 / Self::MAX_BLOCK_SIZE as f32).ceil() as u32;

        for i in 0..num_of_blocks {
            if i == num_of_blocks - 1 && self.length % block_length > 0 {
                block_length = self.length % block_length;
            }

            let begin = i * Self::MAX_BLOCK_SIZE;
            let end = begin + block_length;
            let block = Block::new(/*i,*/ begin, end, block_length);

            block_queue.push(block);
        }

        block_queue
    }

    pub fn check_integrity(&self, hash: PieceHash) -> Result<(), IntegrityError> {
        if self.hash.eq(&hash) {
            Ok(())
        } else {
            Err(IntegrityError(&self.hash, hash))
        }
    }
}

impl Block {
    pub fn new(/*index: u32,*/ begin: u32, end: u32, length: u32) -> Block {
        Block {
            // index,
            begin,
            end,
            length,
            data: None
        }
    }
}

pub fn hash_sha1(v: &Vec<u8>) -> Vec<u8> {
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
pub struct IntegrityError<'a>(&'a PieceHash, PieceHash);

impl<'a> fmt::Display for IntegrityError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Incorrect piece hash, Expected hash: {:?} but got {:?}", self.0, self.1)
    }
}
impl<'a> Error for IntegrityError<'a> {}
