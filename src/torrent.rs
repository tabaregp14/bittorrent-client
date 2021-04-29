use std::collections::VecDeque;
use std::error::Error;
use std::{fs, fmt, io};
use std::path::Path;
use std::convert::TryFrom;
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
    pub length: u64, // file size
    files: Option<Vec<TorrentSubFile>>,
    piece_length: u32
}

#[derive(Clone)]
pub struct Piece {
    pub index: u32,
    pub length: u32, // piece size
    pub begin: u32,
    hash: PieceHash,
    end: u32
}

#[derive(Debug, PartialEq)]
pub struct Block {
    // index: u32,
    pub begin: u32,
    pub end: u32,
    pub length: u32,
    pub data: Option<Vec<u8>>
}

impl BencodeTorrent {
    fn get_total_length(&self) -> u64 {
        match self.info.length {
            Some(length) => length,
            None => self.info.files.as_ref()
                .unwrap()
                .iter()
                .fold(0, |acc, file| acc + file.length)
        }
    }
}

impl Torrent {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Torrent, OpenTorrentError> {
        let file = fs::read(path)?;
        let bencode_torrent = serde_bencode::from_bytes::<BencodeTorrent>(&file)?;
        let torrent = Torrent::try_from(bencode_torrent)?;

        Ok(torrent)
    }

    pub fn create_piece_queue(&self) -> VecDeque<Piece> {
        let mut piece_queue = VecDeque::<Piece>::new();
        let piece_length = self.piece_length as u64;
        let mut length = piece_length;

        for (index, hash) in self.pieces.iter().enumerate() {
            // if is last piece and last piece is smaller than piece_length
            if index == self.pieces.len() - 1 && self.length % piece_length > 0 {
                length = self.length % piece_length;
            }

            let piece = Piece::new(index as u32,
                                   (index * piece_length as usize) as u32,
                                   length as u32,
                                   hash.to_owned());

            piece_queue.push_back(piece);
        }

        piece_queue
    }
}

impl TryFrom<BencodeTorrent> for Torrent {
    type Error = serde_bencode::Error;

    fn try_from(bencode: BencodeTorrent) -> Result<Torrent, Self::Error> {
        let info_bytes = serde_bencode::to_bytes(&bencode.info)?;
        let length = bencode.info.length
            .unwrap_or_else(|| bencode.get_total_length());

        Ok(Torrent {
            info_hash: Sha1::digest(&info_bytes).to_vec(),
            name: bencode.info.name,
            announce: bencode.announce,
            files: bencode.info.files,
            length,
            piece_length: bencode.info.piece_length,
            pieces: bencode.info.pieces.chunks(20)
                .map(|s| s.to_vec())
                .collect()
        })
    }
}

impl Piece {
    const MAX_BLOCK_SIZE: u32 = 16384;

    fn new(index: u32, begin: u32, length: u32, hash: PieceHash) -> Self {
        Piece {
            index,
            hash,
            length,
            begin,
            end: begin + length
        }
    }

    pub fn create_block_queue(&self) -> Vec<Block> {
        let mut block_queue = Vec::<Block>::new();
        let mut block_length = Self::MAX_BLOCK_SIZE;
        let num_of_blocks = (self.length as f32 / Self::MAX_BLOCK_SIZE as f32).ceil() as u32;

        for i in 0..num_of_blocks {
            // if is last block and last block is smaller than block_length
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

        write!(f, "Torrent:\n\
                   ----Name: {}\n\
                   ----Files: {:?}\n\
                   ----Size: {}\n\
                   ----Number of pieces: {}\n\
                   ----Size of pieces: {}",
               self.name,
               files_names,
               self.length,
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

#[derive(Debug)]
pub enum OpenTorrentError {
    SerializationError(serde_bencode::Error),
    IOError(io::Error)
}

impl fmt::Display for OpenTorrentError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::SerializationError(e) =>
                write!(f, "{}", e),
            Self::IOError(..) =>
                write!(f, "Error reading file")
        }
    }
}
impl From<serde_bencode::Error> for OpenTorrentError {
    fn from(err: serde_bencode::Error) -> Self {
        Self::SerializationError(err)
    }
}
impl From<io::Error> for OpenTorrentError {
    fn from(err: io::Error) -> Self {
        Self::IOError(err)
    }
}

#[cfg(test)]
mod tests {
    use sha1::{Sha1, Digest};
    use crate::torrent::{Piece, Block};

    #[test]
    fn check_correct_piece_integrity() {
        let data = &[0, 1, 2, 3, 4];
        let hash = Sha1::digest(data).to_vec();
        let piece = Piece::new(0, 0, 30, hash.to_owned());

        assert!(!piece.check_integrity(hash).is_err());
    }

    #[test]
    fn check_wrong_piece_integrity() {
        let data_1 = &[0, 1, 2, 3, 4];
        let data_2 = &[0, 1, 2, 3, 5];
        let hash_1 = Sha1::digest(data_1).to_vec();
        let hash_2 = Sha1::digest(data_2).to_vec();
        let piece = Piece::new(0, 0, 30, hash_1.to_owned());

        assert!(piece.check_integrity(hash_2).is_err());
    }

    #[test]
    fn create_block_queue() {
        let hash = Sha1::digest(&[0, 1, 2, 3, 4]).to_vec();
        let piece = Piece::new(0, 0, (Piece::MAX_BLOCK_SIZE as f32 * 4.5) as u32, hash);
        let block_queue = piece.create_block_queue();
        let mut control_block_queue = vec![];
        for i in 0..5 {
            let begin = Piece::MAX_BLOCK_SIZE * i;
            let mut length = Piece::MAX_BLOCK_SIZE;

            if i == 4 { length = (Piece::MAX_BLOCK_SIZE as f32 * 0.5) as u32 }

            let block = Block::new(begin,
                                   begin + length,
                                   length);

            control_block_queue.push(block);
        }

        assert_eq!(control_block_queue, block_queue);
    }
}