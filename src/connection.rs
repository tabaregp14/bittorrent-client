use std::net::{TcpStream, Ipv4Addr, SocketAddr, IpAddr};
use std::error::Error;
use std::io::{self, Write, Read};
use std::fmt;
use std::fmt::Debug;
use std::string::FromUtf8Error;
use std::time::Duration;
use core::result;
use byteorder::{BigEndian, ByteOrder};
use serde::{Deserialize, Deserializer, de};
use serde::de::Visitor;
use crate::message::Message;
use crate::client::Client;

type Result<T> = result::Result<T, ConnectionError>;

struct PeerVecVisitor;

struct Handshake {
    pstr: String, // protocol identifier ("BitTorrent protocol")
    info_hash: Vec<u8>,
    peer_id: Vec<u8>
}

pub struct Connection {
    stream: TcpStream,
    pub name: String,
    pub chocked: bool,
    pub bitfield: Option<Vec<u8>>
}

#[derive(Deserialize, Clone, Copy)]
pub struct Peer {
    ip: Ipv4Addr,
    port: u16
}

#[derive(Deserialize)]
pub struct TrackerResponse {
    interval: u32,
    #[serde(deserialize_with = "Peer::vec_from_bytes")]
    pub peers: Vec<Peer>
}

impl<'a> Handshake {
    const PROTOCOL_IDENTIFIER: &'a str = "BitTorrent protocol";

    fn new(info_hash: &Vec<u8>, peer_id: &Vec<u8>) -> Handshake {
        Handshake {
            pstr: String::from(Self::PROTOCOL_IDENTIFIER),
            info_hash: info_hash.to_owned(),
            peer_id: peer_id.to_owned()
        }
    }

    fn as_bytes(&self) -> Vec<u8> {
        let mut result = Vec::new();

        result.push(self.pstr.len() as u8);
        result.extend(self.pstr.as_bytes());
        result.extend(&[0; 8]);
        result.extend(&self.info_hash);
        result.extend(&self.peer_id);

        result
    }

    fn from_bytes(b: &[u8]) -> result::Result<Handshake, FromUtf8Error> {
        let pstr_len = 19;
        let pstr = String::from_utf8(b[1..pstr_len + 1].to_vec())?;
        let info_hash = b[pstr_len + 1 + 8..pstr_len + 1 + 8 + 20].to_vec();
        let peer_id = &b[pstr_len + 1 + 8 + 20..];
        let peer_id = peer_id.to_vec();

        Ok(Handshake {
            pstr,
            info_hash,
            peer_id
        })
    }
}

impl Peer {
    fn from_bytes(b: &[u8]) -> Peer {
        let ip = Ipv4Addr::new(b[0], b[1], b[2], b[3]);
        let port = BigEndian::read_u16(&[b[4], b[5]]);

        Peer { ip, port }
    }

    fn vec_from_bytes<'de, D: Deserializer<'de>>(d: D) -> result::Result<Vec<Peer>, D::Error> {
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

    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> result::Result<Self::Value, E> {
        Ok(v.chunks(6).map(Peer::from_bytes).collect())
    }
}

impl Connection {
    pub fn new(client: &Client, peer: Peer) -> Result<Connection> {
        let addr = SocketAddr::from(peer);
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;

        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;

        let mut conn = Connection {
            name: peer.ip.to_string(),
            stream,
            chocked: true,
            bitfield: None
        };

        conn.complete_handshake(client)?;

        Ok(conn)
    }

    pub fn send(&mut self, message: Message) -> io::Result<()> {
        self.stream.write_all(&message.serialize())?;

        Ok(())
    }

    pub fn read(&mut self) -> io::Result<Message> {
        let mut buf = [0; 4];
        let mut stream = &self.stream;
        let mut msg = Vec::new();

        stream.read_exact(&mut buf)?;

        let msg_len = BigEndian::read_u32(&buf);

        stream.take(msg_len as u64).read_to_end(&mut msg)?;

        if msg_len > 0 {
            Ok(Message::new(msg[0], &msg[1..]))
        } else {
            Ok(Message::KeepAlive)
        }
    }

    pub fn has_piece(&self, index: &u32) -> bool {
        let bitfield = self.bitfield.to_owned().expect("Bitfield not found");
        let byte_index = index / 8;
        let offset = index % 8;

        bitfield[byte_index as usize] & (1 << (7 - offset)) != 0
    }

    pub fn set_piece(&mut self, index: &u32) {
        let bitfield = self.bitfield.as_mut().expect("Bitfield not found");
        let byte_index = index / 8;
        let offset = index % 8;

        bitfield[byte_index as usize] |= 1 << (7 - offset);
    }

    fn send_handshake(&mut self, client: &Client) -> io::Result<Handshake> {
        let hs = Handshake::new(&client.torrent.info_hash, &client.id);

        self.stream.write_all(&hs.as_bytes().as_slice())?;

        Ok(hs)
    }

    fn receive_handshake(&mut self) -> Result<Handshake> {
        let mut buf = [0; 68];

        self.stream.read_exact(&mut buf)?;

        let res_hs = Handshake::from_bytes(&buf)?;

        Ok(res_hs)
    }

    fn complete_handshake(&mut self, client: &Client) -> Result<()> {
        let hs = self.send_handshake(client)?;
        let res_hs = self.receive_handshake()?;

        if hs.info_hash.eq(&res_hs.info_hash) {
            Ok(())
        } else {
            Err(ConnectionError::from(WrongHash(hs.info_hash, res_hs.info_hash)))
        }
    }
}

#[derive(Debug)]
pub struct WrongHash(Vec<u8>, Vec<u8>);

impl fmt::Display for WrongHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Incorrect info_hash, Expected info_hash: {:?} but got {:?}", self.0, self.1)
    }
}

impl Error for WrongHash {}

#[derive(Debug)]
pub enum ConnectionError {
    WrongHash(WrongHash),
    IOError(io::Error),
    Utf8Error(FromUtf8Error)
}

impl fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::WrongHash(e) =>
                write!(f, "{}", e),
            Self::Utf8Error(e) =>
                write!(f, "{}", e),
            Self::IOError(e) =>
                write!(f, "{}", e)
        }
    }
}
impl From<WrongHash> for ConnectionError {
    fn from(err: WrongHash) -> Self {
        Self::WrongHash(err)
    }
}
impl From<io::Error> for ConnectionError {
    fn from(err: io::Error) -> Self {
        Self::IOError(err)
    }
}
impl From<FromUtf8Error> for ConnectionError {
    fn from(err: FromUtf8Error) -> Self {
        Self::Utf8Error(err)
    }
}
