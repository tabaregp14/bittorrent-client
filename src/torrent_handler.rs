use std::error::Error;
use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};
use serde_bencode;
use serde_bytes::ByteBuf;
use sha1::{Digest, Sha1};

#[derive(Debug, Deserialize, Serialize)]
struct TorrentInfo {
    name: String,
    length: u64,
    #[serde(rename = "piece length")]
    piece_length: u64,
    pieces: ByteBuf
}

#[derive(Debug, Deserialize)]
struct BencodeTorrent {
    announce: String,
    info: TorrentInfo
}

#[derive(Debug, Deserialize)]
pub struct Torrent {
    name: String,
    pub announce: String,
    pub info_hash: Vec<u8>,
    pub length: u64,
    piece_length: u64
}

impl TorrentInfo {
    fn hash(&self) -> Result<Vec<u8>, serde_bencode::Error> {
        let bytes = serde_bencode::to_bytes(self)?;
        let mut hasher = Sha1::new();

        hasher.input(bytes);

        Ok(hasher.result()
            .to_vec())
    }
}

impl BencodeTorrent {
    fn to_torrent(self) -> Result<Torrent, serde_bencode::Error> {
        Ok(Torrent {
            info_hash: self.info.hash()?,
            name: self.info.name,
            announce: self.announce,
            length: self.info.length,
            piece_length: self.info.piece_length
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


#[cfg(test)]
mod tests {

}