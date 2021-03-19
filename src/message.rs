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

    pub fn serialize(self) -> Vec<u8> {
        let mut len = [0; 4];
        let mut payload = Vec::<u8>::new();
        let mut message = Vec::<u8>::new();

        match self {
            Message::KeepAlive => {},
            Message::Choke => payload.push(0),
            Message::Unchoke => payload.push(1),
            Message::Interested => payload.push(2),
            Message::NotInterested => payload.push(3),
            Message::Have(index) => {
                let mut buf = [0; 4];

                BigEndian::write_u32(&mut buf, index);
                payload.push(4);
                payload.extend(&buf);
            },
            Message::Bitfield(bitfield) => {
                payload.push(5);
                payload.extend(bitfield);
            },
            Message::Request(index, begin, len) => {
                let mut i = [0; 4];
                let mut b = [0; 4];
                let mut l = [0; 4];

                BigEndian::write_u32(&mut i, index);
                BigEndian::write_u32(&mut b, begin);
                BigEndian::write_u32(&mut l, len);
                payload.push(6);
                payload.extend(&i);
                payload.extend(&b);
                payload.extend(&l);
            },
            Message::Piece(index, begin, piece) => {
                let mut i = [0; 4];
                let mut b = [0; 4];

                BigEndian::write_u32(&mut i, index);
                BigEndian::write_u32(&mut b, begin);
                payload.push(7);
                payload.extend(&i);
                payload.extend(&b);
                payload.extend(piece);
            },
            Message::Cancel => payload.push(8)
        }

        BigEndian::write_u32(&mut len, payload.len() as u32);
        message.extend(&len);
        message.extend(payload);

        message
    }
}
