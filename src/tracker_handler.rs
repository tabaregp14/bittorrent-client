use reqwest;
use reqwest::Url;
use std::time::Duration;
use std::error::Error;
use percent_encoding::percent_encode_byte;
use std::net::Ipv4Addr;
use serde::{Deserialize, Deserializer, de};
use serde::de::Visitor;
use core::fmt;
use byteorder::{BigEndian, ByteOrder};
use crate::torrent::Torrent;

struct PeerVecVisitor;
#[derive(Debug, Deserialize)]
pub struct Peer {
    pub ip: Ipv4Addr,
    pub port: u16
}
#[derive(Debug, Deserialize)]
pub struct TrackerResponse {
    interval: u32,
    #[serde(deserialize_with = "Peer::vec_from_bytes")]
    pub peers: Vec<Peer>
}

impl Peer {
    fn from_bytes(b: &[u8]) -> Peer {
        let ip = Ipv4Addr::new(b[0], b[1], b[2], b[3]);
        let port = BigEndian::read_u16(&[b[4], b[5]]);

        Peer { ip, port }
    }

    fn vec_from_bytes<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Peer>, D::Error> {
        d.deserialize_byte_buf(PeerVecVisitor)
    }
}

impl <'de> Visitor<'de> for PeerVecVisitor {
    type Value = Vec<Peer>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("byte array")
    }

    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        Ok(v.chunks(6).map(Peer::from_bytes).collect())
    }
}

pub fn request_peers(torrent: &Torrent, peer_id: &Vec<u8>, port: &u16) -> Result<TrackerResponse, Box<dyn Error>> {
    let url_hash = (&torrent.info_hash)
        .into_iter()
        .map(|b| percent_encode_byte(*b))
        .collect::<String>();
    let peer_id_es = peer_id.into_iter()
        .map(|b| percent_encode_byte(*b))
        .collect::<String>();
    let base_url = format!("{}?info_hash={}&peer_id={}", torrent.announce, url_hash, peer_id_es);
    let url = Url::parse_with_params(base_url.as_str(),
                                     &[("port", port.to_string()),
                                         ("uploaded", "0".to_string()),
                                         ("downloaded", "0".to_string()),
                                         ("compact", "1".to_string()),
                                         ("left", torrent.calculate_length().to_string())])?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;
    let mut res = client.get(url)
        .send()?;
    let mut buf = Vec::new();

    res.copy_to(&mut buf)?;

    let tracker_response = serde_bencode::from_bytes::<TrackerResponse>(&buf.as_slice())?;

    Ok(tracker_response)
}


#[cfg(test)]
mod tests {

}
