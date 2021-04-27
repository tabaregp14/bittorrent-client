use std::thread::JoinHandle;
use std::net::{TcpStream, SocketAddr};
use std::time::Duration;
use rand::Rng;
use crate::connection::{Connection, ConnectionError};
use crate::tracker_handler::Peer;

pub struct Client {
    pub id: Vec<u8>,
    pub port: u16,
    pub info_hash: Vec<u8>,
    pub uploaded: u32,
    pub downloaded: u32,
    pub workers: Vec<JoinHandle<()>>
}

impl Client {
    const PORT: u16 = 6881;

    pub fn new(info_hash: &Vec<u8>) -> Client {
        Client {
            id: Self::generate_random_id(),
            port: Self::PORT,
            info_hash: info_hash.to_owned(),
            uploaded: 0,
            downloaded: 0,
            workers: Vec::new()
        }
    }

    pub fn connect(&self, peer: Peer) -> Result<Connection, ConnectionError> {
        let addr = SocketAddr::from(peer);
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;

        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;

        let mut conn = Connection::new(stream, peer);

        conn.complete_handshake(self)?;

        Ok(conn)
    }

    fn generate_random_id() -> Vec<u8> {
        let id = rand::thread_rng().gen::<[u8; 20]>().to_vec();

        id
    }
}
