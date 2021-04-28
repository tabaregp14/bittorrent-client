use std::time::Duration;
use std::net::{Ipv4Addr, SocketAddr, IpAddr};
use core::fmt;
use reqwest::Url;
use serde::{Deserialize, Deserializer, de};
use serde::de::Visitor;
use byteorder::{BigEndian, ByteOrder};
use crate::torrent::Torrent;
use crate::utils::url_encode;
use crate::client::Client;

struct PeerVecVisitor;

#[derive(Deserialize, Clone, Copy)]
pub struct Peer {
    pub ip: Ipv4Addr,
    port: u16
}

pub struct Tracker;

#[derive(Deserialize)]
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
    pub fn send_request(torrent: &Torrent, client: &Client) -> Result<TrackerResponse, TrackerError> {
        let mut buf = Vec::new();
        let url = Self::parse_url(&torrent, &client);
        let req_client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()?;
        let mut res = req_client.get(url)
            .send()?;

        res.copy_to(&mut buf)?;

        let tracker_response = serde_bencode::from_bytes::<TrackerResponse>(&buf.as_slice())?;

        Ok(tracker_response)
    }

    fn parse_url(torrent: &Torrent, client: &Client) -> Url {
        let url_hash = url_encode(&torrent.info_hash);
        let url_peer_id = url_encode(&client.id);
        let base_url = format!("{}?info_hash={}&peer_id={}", torrent.announce, url_hash, url_peer_id);
        let url_params = [
            ("port", client.port.to_string()),
            ("uploaded", client.uploaded.to_string()),
            ("downloaded", client.downloaded.to_string()),
            ("compact", "1".to_string()),
            ("left", torrent.calculate_length().to_string())
        ];
        let url = Url::parse_with_params(base_url.as_str(),&url_params).unwrap();

        url
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
