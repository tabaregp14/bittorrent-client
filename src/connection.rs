use std::net::{TcpStream, SocketAddr};
use std::time::Duration;
use std::error::Error;
use std::io::{self, Write, Read};
use std::fmt;
use std::string::FromUtf8Error;
use byteorder::{BigEndian, ByteOrder};
use crate::tracker_handler::Peer;
use crate::message::Message;
use std::fmt::Debug;
use core::result;

type Result<T> = result::Result<T, ConnectionError>;

struct Handshake {
    pstr: String, // protocol identifier ("BitTorrent protocol")
    info_hash: Vec<u8>,
    peer_id: Vec<u8>
}

pub struct Connection {
    pub stream: TcpStream,
    pub chocked: bool,
    pub bitfield: Option<Vec<u8>>,
    pub peer: Peer,
    info_hash: Vec<u8>,
    client_peer_id: Vec<u8>
}

impl<'a> Handshake {
    const PROTOCOL_IDENTIFIER: &'a str = "BitTorrent protocol";

    fn new(info_hash: Vec<u8>, peer_id: Vec<u8>) -> Handshake {
        Handshake {
            pstr: String::from(Self::PROTOCOL_IDENTIFIER),
            info_hash,
            peer_id
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

impl Connection {
    pub fn connect(peer: Peer, info_hash: &Vec<u8>, client_peer_id: &Vec<u8>) -> Result<Connection> {
        let addr = SocketAddr::from(peer);
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;
        let mut conn = Connection {
            stream,
            chocked: true,
            bitfield: None,
            peer,
            info_hash: info_hash.to_owned(),
            client_peer_id: client_peer_id.to_owned()
        };

        conn.stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        conn.stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        conn.complete_handshake()?;

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

    fn send_handshake(&mut self) -> io::Result<Handshake> {
        let hs = Handshake::new(self.info_hash.to_owned(), self.client_peer_id.to_owned());

        self.stream.write_all(&hs.as_bytes().as_slice())?;

        Ok(hs)
    }

    fn receive_handshake(&mut self) -> Result<Handshake> {
        let mut buf = [0; 68];

        self.stream.read_exact(&mut buf)?;

        let res_hs = Handshake::from_bytes(&buf)?;

        Ok(res_hs)
    }

    fn complete_handshake(&mut self) -> Result<Handshake> {
        let hs = self.send_handshake()?;
        let res_hs = self.receive_handshake()?;

        if hs.info_hash.eq(&res_hs.info_hash) {
            Ok(res_hs)
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
            ConnectionError::WrongHash(e) =>
                write!(f, "{}", e),
            ConnectionError::Utf8Error(e) =>
                write!(f, "{}", e),
            ConnectionError::IOError(e) =>
                write!(f, "{}", e)
        }
    }
}
impl From<WrongHash> for ConnectionError {
    fn from(err: WrongHash) -> ConnectionError {
        ConnectionError::WrongHash(err)
    }
}
impl From<io::Error> for ConnectionError {
    fn from(err: io::Error) -> ConnectionError {
        ConnectionError::IOError(err)
    }
}
impl From<FromUtf8Error> for ConnectionError {
    fn from(err: FromUtf8Error) -> ConnectionError {
        ConnectionError::Utf8Error(err)
    }
}
