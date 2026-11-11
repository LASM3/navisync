# Navisync

Navisync is a small Rust CLI and daemon for syncing music from a Navidrome/Subsonic-compatible server to a local folder. It can log in, inspect your server library, search albums/tracks/artists, download tracks, and keep an eye on what is already synced.

## Features

- Log in with username/password and use a saved JWT token
- Show server/user stats
- List albums, artists, tracks, and search results
- Download music into a local directory organized by artist/album
- Check for missing or new tracks compared to a cached snapshot
- Run a daemon loop to periodically sync changes
- Cache API responses and synced track metadata in the local `cache/` directory

## Installing from release

Download the latest prebuilt binary from the [Releases](https://github.com/LASM3/navisync/releases) page.

Create a configuration file from the example on the source and edit it
```toml
server = "http://server:port"
directory = "/home/you/Music"
pref_format = "FLAC"
refresh = 3600000
username = "your-user"
token = ""
```

## Building from source

1. Clone the repository:

```bash
git clone https://github.com/LASM3/navisync.git
cd navisync
```

2. Run the project:

```bash
cargo run -- help
```

3. Copy the example config and edit it:

```bash
cp example-settings.toml settings.toml
```

4. Edit `settings.toml` to point at your server and local music directory:

```toml
server = "http://server:port"
directory = "/home/you/Music"
pref_format = "FLAC"
refresh = 3600000
username = "your-user"
token = ""
```

Notes:
- `directory` is the destination root used for downloads.
- `pref_format` is optional and controls the preferred output format for syncs.
- `refresh` is the daemon polling interval in milliseconds.
- `username` and `token` are optional if you log in interactively.

5. (optional) you can install navisync to your path with `cargo install --path .`. Then you can run stuff like `navisync --daemon`

6. (optional) you can also make a custom systemd service like i do: `navisync.service`:
```
[Unit]
Description=Navisync Service
After=network.target

[Service]
Type=simple
ExecStart=%h/.cargo/bin/navisync -d
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=default.target
```

### Commands

- `stats`: show server and account details
- `albums -l <n>`: list albums from the server
- `tracks -l <n>`: list random tracks
- `artists -l <n>`: list artists
- `search <query> -l <n>`: search songs, albums, and artists
- `sync`: download tracks to the configured directory
- `sync -f <format>`: override the configured preferred format
- `check`: compare server contents to the cached snapshot and local sync state
- `-d` / `--daemon`: run periodic sync checks using the configured refresh interval

## Credits
- Created and maintained by [lasm3](https://github.com/LASM3)

## Donations
If this tool saved you time or helped your workflow, support further development:

[![PayPal](https://img.shields.io/badge/Donate-PayPal-00457C?style=flat-square&logo=paypal)](https://paypal.me/lasm3)
[![Liberapay](https://img.shields.io/badge/Donate-Liberapay-F6C915?style=flat-square&logo=liberapay)](https://liberapay.com/lasm3/)

## License 
This project is under the GNU GPLv3 License, check the [License](LICENSE) file.