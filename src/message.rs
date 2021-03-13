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
                let begin = BigEndian::read_u32(&payload[4..8]);
                let length = BigEndian::read_u32(&payload[8..]);

                Message::Request(index, begin, length)
            },
            7 => {
                let index = BigEndian::read_u32(&payload[..4]);
                let begin = BigEndian::read_u32(&payload[4..8]);
                let piece = payload[8..].to_vec();

                Message::Piece(index, begin, piece)
            },
            8 => Message::Cancel,
            _ => panic!("Bad message ID: {}", id)
        }
    }
}


#[cfg(test)]
mod tests {

}
