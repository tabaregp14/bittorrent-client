use std::time::Duration;
use std::error::Error;
use std::net::{Ipv4Addr, SocketAddr, IpAddr};
use core::fmt;
use reqwest::Url;
use reqwest::blocking::Client;
use serde::{Deserialize, Deserializer, de};
use serde::de::Visitor;
use byteorder::{BigEndian, ByteOrder};
use crate::torrent::Torrent;
use crate::utils::url_encode;

struct PeerVecVisitor;

#[derive(Deserialize, Clone, Copy)]
pub struct Peer {
    pub ip: Ipv4Addr,
    port: u16
}

pub struct Tracker;

#[derive(Deserialize)]
struct TrackerResponse {
    interval: u32,
    #[serde(deserialize_with = "Peer::vec_from_bytes")]
    peers: Vec<Peer>
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

impl From<Peer> for SocketAddr {
    fn from(peer: Peer) -> SocketAddr {
        SocketAddr::new(IpAddr::from(peer.ip), peer.port)
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

impl Tracker {
    // TODO: add peer id prefix
    pub fn request_peers(torrent: &Torrent, peer_id: &Vec<u8>, port: &u16) -> Result<Vec<Peer>, Box<dyn Error>> {
        let url_hash = url_encode(&torrent.info_hash);
        let url_peer_id = url_encode(peer_id);
        let base_url = format!("{}?info_hash={}&peer_id={}", torrent.announce, url_hash, url_peer_id);
        let url_params = [
            ("port", port.to_string()),
            ("uploaded", "0".to_string()),
            ("downloaded", "0".to_string()),
            ("compact", "1".to_string()),
            ("left", torrent.calculate_length().to_string())
        ];
        let url = Url::parse_with_params(base_url.as_str(),&url_params)?;
        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()?;
        let mut res = client.get(url)
            .send()?;
        let mut buf = Vec::new();

        res.copy_to(&mut buf)?;

        let tracker_response = serde_bencode::from_bytes::<TrackerResponse>(&buf.as_slice())?;

        Ok(tracker_response.peers)
    }
}
