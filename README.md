# bittorrent-client
Simple BitTorrent client written in Rust.

For now, it only downloads torrents with a single file, it can't seed nor resume a partial download (it restarts the download).

## Usage
`bittorrent-client <torrent file path> [out path]`

## TODO
- Seeding
- Downloading torrents with multiple files
- Resuming downloads