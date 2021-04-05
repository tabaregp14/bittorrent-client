use std::sync::{Arc, Mutex};
use std::{thread, io, fmt};
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::thread::JoinHandle;
use std::time::Duration;
use crate::message::Message;
use crate::connection::Connection;
use crate::torrent::{Piece, hash_sha1, Block, Torrent, IntegrityError};

pub struct DownloaderWorker {
    name: String,
    conn: Connection,
    torrent_state: Arc<TorrentState>
}

pub struct TorrentState {
    piece_queue: Mutex<Vec<Piece>>,
    done_pieces: Mutex<u32>,
    length: u32,
    file: Mutex<File>
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
    pub fn new(torrent_state: Arc<TorrentState>, conn: Connection) -> DownloaderWorker {
        DownloaderWorker {
            name: (&conn.peer.ip).to_string(),
            conn,
            torrent_state
        }
    }

    pub fn start(mut self) -> JoinHandle<()> {
        thread::Builder::new()
            .name(format!("{}", &self.name))
            .spawn(move || {
                while self.conn.chocked {
                    match self.conn.read().unwrap() {
                        Message::Bitfield(bitfield) => {
                            self.conn.bitfield = Some(bitfield);
                            self.conn.send(Message::Unchoke).unwrap();
                            self.conn.send(Message::Interested).unwrap();
                        },
                        Message::Unchoke => {
                            self.conn.chocked = false;

                            &self.download();
                        },
                        _ => {}
                    }
                }
            })
            .unwrap()
    }

    fn download(&mut self) {
        while !self.torrent_state.is_done() {
            match self.torrent_state.get_piece_from_queue() {
                Some(work_piece) => {
                    if !self.conn.has_piece(&work_piece.index) {
                        println!("Thread [{:?}]: Peer doesn't have piece {}", thread::current().name().unwrap(), &work_piece.index);
                        let mut pieces_queue = self.torrent_state.piece_queue.lock().unwrap();

                        pieces_queue.push(work_piece);

                        // Prevent deadlock
                        drop(pieces_queue);
                        thread::sleep(Duration::from_secs(3));

                        continue;
                    }

                    let piece_result = self.try_download_piece(&work_piece);

                    match piece_result {
                        Ok(piece) => {
                            let mut done_pieces = self.torrent_state.done_pieces.lock().unwrap();
                            let mut file = self.torrent_state.file.lock().unwrap();

                            piece.copy_to_file(&mut *file).unwrap();
                            *done_pieces += 1;

                            println!("Thread [{:?}]: Piece {} finished. Done pieces: {} / {}",thread::current().name().unwrap() , &work_piece.index, &done_pieces, &self.torrent_state.length);
                        }
                        Err(e) => {
                            println!("Thread [{:?}] ERROR: {:?}", thread::current().name().unwrap(), e);
                            let mut pieces_queue = self.torrent_state.piece_queue.lock().unwrap();

                            pieces_queue.push(work_piece.to_owned());

                            // FIXME: break only on specific errors
                            println!("Disconnecting...");
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
                while state.requested_blocks.len() < PieceState::MAX_CONCURRENT_REQUESTS as usize && !state.block_queue.is_empty() {
                    match state.block_queue.pop() {
                        Some(b) => state.send_request(b, &mut self.conn)?,
                        None => println!("Empty block queue")
                    }
                }
            }

            match state.read_message(&mut self.conn)? {
                Some(block) => state.store_in_buffer(block),
                None => println!("Thread [{:?}]: MESSAGE IS NOT A PIECE", thread::current().name().unwrap())
            }
        }

        piece.check_integrity(hash_sha1(&state.buf))?;

        Ok(state)
    }
}

impl TorrentState {
    pub const MAX_CONCURRENT_PEERS: u8 = 20;

    pub fn new(torrent: &Torrent) -> TorrentState {
        let file = File::create(&torrent.name).unwrap();

        file.set_len(torrent.calculate_length()).unwrap();

        TorrentState {
            done_pieces: Mutex::new(0),
            piece_queue: Mutex::new(torrent.create_piece_queue()),
            length: torrent.pieces.len() as u32,
            file: Mutex::new(file)
        }
    }

    pub fn is_done(&self) -> bool {
        let done_pieces = self.done_pieces.lock().unwrap();

        if *done_pieces >= self.length {
            return true;
        }

        false
    }

    fn get_piece_from_queue(&self) -> Option<Piece> {
        let mut piece_queue = self.piece_queue.lock().unwrap();

        piece_queue.pop()
    }
}

impl PieceState {
    const MAX_CONCURRENT_REQUESTS: u8 = 5;

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

    fn send_request(&mut self, block: Block, conn: &mut Connection) -> Result<(), io::Error> {
        conn.send(Message::Request(self.index, block.begin, block.length))?;
        self.requested_blocks.push(block);

        Ok(())
    }

    fn read_message(&mut self, conn: &mut Connection) -> Result<Option<Block>, io::Error> {
        match conn.read()? {
            Message::Piece(index, begin, block_data) => {
                if index != self.index {
                    println!("Thread [{:?}]: Expected piece ID {} but got {}", thread::current().name().unwrap(), &self.index, &index);

                    return Ok(None);
                }

                let block_index = self.requested_blocks.iter().position(|b| b.begin == begin);

                match block_index {
                    Some(block_index) => {
                        let mut block = self.requested_blocks.remove(block_index);

                        self.blocks_done += 1;
                        block.data = Some(block_data);

                        Ok(Some(block))
                    },
                    None => {
                        println!("Thread [{:?}]: Received block was not requested", thread::current().name().unwrap());

                        Ok(None)
                    }
                }
            },
            Message::Have(index) => {
                println!("Have: {}", &index);
                conn.set_piece(&index);

                Ok(None)
            },
            Message::Choke => {
                println!("Thread [{:?}]: Choked", thread::current().name().unwrap());
                conn.chocked = true;

                Ok(None)
            },
            Message::Unchoke => {
                println!("Thread [{:?}]: Unchoked", thread::current().name().unwrap());
                conn.chocked = false;

                Ok(None)
            },
            _ => {
                println!("Thread [{:?}]: Other message", thread::current().name().unwrap());

                Ok(None)
            }
        }
    }

    // TODO: handle Option
    fn store_in_buffer(&mut self, block: Block) {
        self.buf.splice(block.begin as usize..block.end as usize, block.data.unwrap());
    }

    fn copy_to_file(&self, file: &mut File) -> Result<(), io::Error> {
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
            DownloadPieceError::WrongHash(e) =>
                write!(f, "{}", e),
            DownloadPieceError::IOError(..) =>
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
