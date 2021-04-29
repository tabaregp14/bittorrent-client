use std::net::{Ipv4Addr, SocketAddr, IpAddr};
use core::fmt;
use serde::{Deserialize, Deserializer, de};
use serde::de::Visitor;
use byteorder::{BigEndian, ByteOrder};

struct PeerVecVisitor;

#[derive(Deserialize, Clone, Copy)]
pub struct Peer {
    pub ip: Ipv4Addr,
    port: u16
}

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

    pub fn vec_from_bytes<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Peer>, D::Error> {
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
