use std::net::{TcpStream, SocketAddr, IpAddr};
use crate::tracker_handler::Peer;
use std::time::Duration;
use std::error::Error;
use std::io::{self, Write, Read};
use std::convert::TryFrom;
use std::string::FromUtf8Error;

#[derive(Debug)]
pub struct Handshake {
    pstr: String,
    info_hash: Vec<u8>,
    peer_id: Vec<u8>
}

pub struct Connection {
    pub stream: TcpStream,
    chocked: bool,
    peer: Peer,
    info_hash: Vec<u8>,
    peer_id: Vec<u8>
}

impl Handshake {
    fn new(info_hash: Vec<u8>, peer_id: Vec<u8>) -> Handshake {
        Handshake {
            pstr: String::from("BitTorrent protocol"),
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

    fn from_bytes(b: &[u8]) -> Result<Handshake, FromUtf8Error> {
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
    pub fn connect(peer: Peer, info_hash: Vec<u8>, peer_id: Vec<u8>) -> Result<Connection, io::Error> {
        let addr = SocketAddr::new(IpAddr::from(peer.ip), peer.port);
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;

//    conn.set_write_timeout(Some(Duration::from_secs(5)))?;
//    conn.set_read_timeout(Some(Duration::from_secs(5)))?;

        Ok(Connection {
            stream,
            chocked: true,
            peer,
            info_hash,
            peer_id
        })
    }

    pub fn complete_handshake(&mut self) -> Result<Handshake, Box<dyn Error>> {
        let hs = self.send_handshake()?;
        let res_hs = self.receive_handshake()?;

        if hs.info_hash.eq(&res_hs.info_hash) {
            println!("Successful handshake.");

            Ok(res_hs)
        } else {
            println!("Expected info_hash: {:?} but got {:?}", hs.info_hash, res_hs.info_hash);

            Err(Box::try_from("Incorrect info_hash.").unwrap())
        }
    }

    fn send_handshake(&mut self) -> Result<Handshake, io::Error> {
        let hs = Handshake::new(self.info_hash.to_owned(), self.peer_id.to_owned());

        self.stream.write_all(&hs.as_bytes().as_slice())?;

        Ok(hs)
    }

    fn receive_handshake(&mut self) -> Result<Handshake, Box<dyn Error>> {
        let mut buf = [0; 68];

        self.stream.read_exact(&mut buf)?;

        let res_hs = Handshake::from_bytes(&buf)?;

        Ok(res_hs)
    }
}


#[cfg(test)]
mod tests {

}
