use std::sync::Arc;
use std::{thread, io, fmt};
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::thread::JoinHandle;
use sha1::{Sha1, Digest};
use crate::message::Message;
use crate::connection::Connection;
use crate::torrent::{Piece, Block, IntegrityError};
use crate::println_thread;
use crate::client::Client;

pub struct DownloaderWorker {
    conn: Connection,
    client: Arc<Client>
}

struct PieceState {
    index: u32,
    begin: u32,
    requested_blocks: Vec<Block>,
    blocks_done: u8,
    block_queue: Vec<Block>,
    buf: Vec<u8>
}

impl DownloaderWorker {
    pub fn new(client: Arc<Client>, conn: Connection) -> DownloaderWorker {
        DownloaderWorker {
            conn,
            client
        }
    }

    pub fn start(mut self) -> JoinHandle<()> {
        thread::Builder::new()
            .name(format!("{}", &self.conn.name))
            .spawn(move || {
                while self.conn.chocked {
                    match self.conn.read() {
                        Ok(msg) => self.interpret_message(msg).unwrap(),
                        Err(_) => break
                    }
                }
            }).expect("Error starting worker.")
    }

    fn download(&mut self) {
        while !self.client.torrent.is_done() {
            match self.client.torrent.get_piece_from_queue() {
                Some(work_piece) => {
                    if !self.conn.has_piece(&work_piece.index) {
                        self.client.torrent.push_piece_to_queue(work_piece);

                        continue;
                    }

                    match self.try_download_piece(&work_piece) {
                        Ok(piece) => {
                            let mut done_pieces = self.client.get_done_pieces();
                            let mut file = self.client.get_file();

                            piece.copy_to_file(&mut *file).unwrap();
                            *done_pieces += 1;

                            println!("Piece {} finished. Pieces done: {} / {} from {} peers",
                                     &work_piece.index,
                                     &done_pieces,
                                     &self.client.torrent.total_pieces,
                                     Arc::strong_count(&self.client) - 1);
                        }
                        Err(_) => {
                            self.client.torrent.push_piece_to_queue(work_piece/*.to_owned()*/);

                            // FIXME: break only on specific errors
                            break;
                        }
                    }
                },
                None => break
            }
        }
    }

    fn try_download_piece<'a>(&'a mut self, piece: &'a Piece) -> Result<PieceState, DownloadPieceError> {
        let mut state = PieceState::new(piece);

        while !state.block_queue.is_empty() || !state.requested_blocks.is_empty() {
            if !self.conn.chocked {
                while state.can_send_request() && !state.block_queue.is_empty() {
                    match state.block_queue.pop() {
                        Some(b) => state.send_request(b, &mut self.conn)?,
                        None => println!("Empty block queue")
                    }
                }
            }

            match state.read_message(&mut self.conn)? {
                Some(block) => state.store_block_in_buffer(block),
                None => continue
            }
        }

        piece.check_integrity(Sha1::digest(&state.buf).to_vec())?;

        Ok(state)
    }

    fn interpret_message(&mut self, message: Message) -> io::Result<()> {
        match message {
            Message::Bitfield(bitfield) => {
                self.conn.bitfield = Some(bitfield);

                self.conn.send(Message::Unchoke)?;
                self.conn.send(Message::Interested)?;
            },
            Message::Unchoke => {
                self.conn.chocked = false;

                &self.download();
            },
            Message::Have(index) => self.conn.set_piece(&index),
            _ => {}
        }

        Ok(())
    }
}

impl PieceState {
    const MAX_CONCURRENT_REQUESTS: usize = 5;

    fn new(piece: &Piece) -> PieceState {
        PieceState {
            index: piece.index,
            begin: piece.begin,
            requested_blocks: Vec::new(),
            buf: vec![0; piece.length as usize],
            block_queue: piece.create_block_queue(),
            blocks_done: 0
        }
    }

    fn send_request(&mut self, block: Block, conn: &mut Connection) -> io::Result<()> {
        conn.send(Message::Request(self.index, block.begin, block.length))?;
        self.requested_blocks.push(block);

        Ok(())
    }

    fn read_message(&mut self, conn: &mut Connection) -> io::Result<Option<Block>> {
        match conn.read()? {
            Message::Piece(index, begin, block_data) => {
                if index != self.index {
                    println_thread!("Expected piece ID {} but got {}", &self.index, &index);

                    return Ok(None);
                }

                let block_index = self.requested_blocks.iter()
                    .position(|b| b.begin == begin);

                match block_index {
                    Some(block_index) => {
                        let mut block = self.requested_blocks.remove(block_index);

                        self.blocks_done += 1;
                        block.data = Some(block_data);

                        Ok(Some(block))
                    },
                    None => {
                        println_thread!("Received block was not requested");

                        Ok(None)
                    }
                }
            },
            Message::Have(index) => {
                conn.set_piece(&index);

                Ok(None)
            },
            Message::Choke => {
                conn.chocked = true;

                Ok(None)
            },
            Message::Unchoke => {
                conn.chocked = false;

                Ok(None)
            },
            _ => Ok(None)
        }
    }

    fn can_send_request(&self) -> bool {
        self.requested_blocks.len() < PieceState::MAX_CONCURRENT_REQUESTS
    }

    // TODO: handle Option
    fn store_block_in_buffer(&mut self, block: Block) {
        self.buf.splice(block.begin as usize..block.end as usize, block.data.unwrap());
    }

    fn copy_to_file(&self, file: &mut File) -> io::Result<()> {
        file.seek(SeekFrom::Start(self.begin as u64))?;
        file.write_all(&self.buf)?;

        Ok(())
    }
}

#[derive(Debug)]
enum DownloadPieceError<'a> {
    WrongHash(IntegrityError<'a>),
    IOError(io::Error)
}

impl<'a> fmt::Display for DownloadPieceError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::WrongHash(e) =>
                write!(f, "{}", e),
            Self::IOError(..) =>
                write!(f, "Error sending message")
        }
    }
}
impl<'a> From<IntegrityError<'a>> for DownloadPieceError<'a> {
    fn from(err: IntegrityError) -> DownloadPieceError {
        DownloadPieceError::WrongHash(err)
    }
}
impl<'a> From<io::Error> for DownloadPieceError<'a> {
    fn from(err: io::Error) -> DownloadPieceError<'a> {
        DownloadPieceError::IOError(err)
    }
}
