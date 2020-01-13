use std::net::TcpStream;
use std::io::{self, Read};
use byteorder::{BigEndian, ByteOrder};

#[derive(Debug)]
pub enum Message {
    KeepAlive,
    Choke,
    Unchoke,
    Interested,
    NotInterested,
    Have(u32),
    Bitfield(Vec<u8>),
    Request(u32, u32, u32),
    Piece(u32, u32, Vec<u8>),
    Cancel
}

impl Message {
    pub fn new(id: u8, payload: &[u8]) -> Message {
        match id {
            0 => Message::Choke,
            1 => Message::Unchoke,
            2 => Message::Interested,
            3 => Message::NotInterested,
            4 => Message::Have(BigEndian::read_u32(payload)),
            5 => Message::Bitfield(payload.to_vec()),
            6 => {
                let index = BigEndian::read_u32(&payload[..4]);
                let begin = BigEndian::read_u32(&payload[5..8]);
                let length = BigEndian::read_u32(&payload[9..]);

                Message::Request(index, begin, length)
            },
            7 => {
                let index = BigEndian::read_u32(&payload[..4]);
                let begin = BigEndian::read_u32(&payload[5..8]);
                let piece = payload[9..].to_vec();

                Message::Piece(index, begin, piece)
            },
            8 => Message::Cancel,
            _ => panic!("Bad message ID: {}", id)
        }
    }

    pub fn read(mut conn: &TcpStream) -> Result<Message, io::Error> {
        let mut msg_len = [0; 4];

        conn.read_exact(&mut msg_len)?;

        let msg_len = BigEndian::read_u32(&msg_len);
        let mut msg = Vec::new();

        conn.take(msg_len as u64).read_to_end(&mut msg)?;

        if msg_len > 0 {
            Ok(Message::new(msg[0], &msg[1..]))
        } else {
            Ok(Message::KeepAlive)
        }
    }
}


#[cfg(test)]
mod tests {

}
