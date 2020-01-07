use std::io;
use std::path::Path;
use std::error::Error;
use crate::torrent_handler::Torrent;

mod torrent_handler;

fn main() {
    let input = read_input().unwrap();
    let path = Path::new(&input);
    let torrent = Torrent::open(path).unwrap();

    println!("{:?}", torrent);
}

fn read_input() -> Result<String, Box<dyn Error>> {
    let mut input = String::new();

    println!("Path of the torrent");
    io::stdin().read_line(&mut input)?;

    input = input.trim().parse()?;

    Ok(input)
}
