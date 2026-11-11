mod api;
mod cache;
mod settings;

use std::io::{self, Write};

use clap::{Parser, Subcommand};
use rpassword;

#[derive(Parser, Debug)]
#[command(
    name = "navisync",
    version,
    about,
    after_help = EXAMPLES
)]

struct Args {
    #[arg(short, long)]
    daemon: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

const EXAMPLES: &str = "Examples:
  navisync                  Interactive mode (token login, stats, albums)
  navisync stats            Show server and user stats
  navisync albums -l 10     List at most 10 albums
  navisync tracks -l 5      List at most 5 random tracks
  navisync artists -l 20    List at most 20 artists
  navisync search adele -l 2   Search 'adele', at most 2 results per type
  navisync sync             Download all music to the directory in settings.toml
  navisync sync -f MP3 -l 50   Download at most 50 tracks, transcoded to MP3 (overrides settings)
  navisync check            Compare the server with the local library (new and missing tracks)
  navisync -d               Run the daemon: recheck every 'refresh' ms and sync new tracks";

#[derive(Subcommand, Debug)]
enum Command {
    /// Show server and user stats
    Stats,
    /// List albums from the server
    Albums {
        /// Maximum number of albums to fetch
        #[arg(short, long)]
        limit: Option<i64>,
    },
    /// List random tracks from the server
    Tracks {
        /// Maximum number of tracks to fetch
        #[arg(short, long)]
        limit: Option<i64>,
    },
    /// List artists from the server
    Artists {
        /// Maximum number of artists to fetch
        #[arg(short, long)]
        limit: Option<i64>,
    },
    /// Search tracks, albums and artists
    Search {
        /// Search query
        query: String,
        /// Maximum number of results per type (artists, albums, songs)
        #[arg(short, long)]
        limit: Option<i64>,
    },
    /// Download music from the server to the directory in settings.toml
    Sync {
        #[arg(short, long)]
        format: Option<String>,
        /// Maximum number of tracks to download
        #[arg(short, long)]
        limit: Option<i64>,
    },
    /// Check the server for new tracks and report what is not downloaded yet
    Check,
}

fn read_line(prompt: &str) -> String {
    print!("{prompt} ");
    io::stdout().flush().expect("failed to flush stdout");
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .expect("failed to read line");
    line.trim_end().to_string()
}

fn read_password(prompt: &str) -> String {
    print!("{prompt} ");
    io::stdout().flush().expect("failed to flush stdout");
    rpassword::read_password().expect("failed to read password")
}

async fn client_from_settings() -> api::NavidromeClient {
    let mut settings = settings::load().expect("failed to load settings");
    let server = settings.server.clone();

    if let Some(token) = settings
        .token
        .clone()
        .filter(|t| !t.is_empty() && !api::token_expired(t))
    {
        let username = settings.username.clone().unwrap_or_default();
        println!("Reusing stored token for '{username}'");
        return api::NavidromeClient::new_with_token(&server, &username, &token);
    }

    let username = read_line("Enter Navidrome username:");
    let password = read_password("Enter Navidrome password:");

    match api::login(&server, &username, &password).await {
        Ok(token) => {
            settings.username = Some(username.clone());
            settings.token = Some(token.clone());
            settings::save(&settings).unwrap_or_else(|e| eprintln!("failed to save token: {e}"));
            println!("Login successful, token saved to settings.toml");
            api::NavidromeClient::new_with_token(&server, &username, &token)
        }
        Err(e) => {
            eprintln!("login failed ({e}); falling back to username/password auth");
            api::NavidromeClient::new(&server, &username, &password)
        }
    }
}

fn print_json(value: &serde_json::Value) {
    match serde_json::to_string_pretty(value) {
        Ok(s) => println!("{s}"),
        Err(e) => println!("{value:?} (failed to pretty-print: {e})"),
    }
}

fn print_tracks(tracks: &[api::Track]) {
    if tracks.is_empty() {
        println!("No tracks found");
        return;
    }
    for track in tracks {
        println!(
            "- {} - {} [{album}] {size}",
            track.title.as_deref().unwrap_or("(unknown title)"),
            track.artist.as_deref().unwrap_or("(unknown artist)"),
            album = track.album.as_deref().unwrap_or("(unknown album)"),
            size = track
                .size
                .map(|s| format!("({s} bytes)"))
                .unwrap_or_default()
        );
    }
}

fn print_search(results: &serde_json::Value) {
    let Some(result) = results.get("searchResult2") else {
        println!("No search results");
        return;
    };

    let empty = Vec::new();
    let artists: Vec<&serde_json::Value> = result
        .get("artist")
        .and_then(|a| a.as_array())
        .unwrap_or(&empty)
        .iter()
        .collect();
    let albums: Vec<&serde_json::Value> = result
        .get("album")
        .and_then(|a| a.as_array())
        .unwrap_or(&empty)
        .iter()
        .collect();
    let songs: Vec<&serde_json::Value> = result
        .get("song")
        .and_then(|a| a.as_array())
        .unwrap_or(&empty)
        .iter()
        .collect();

    if artists.is_empty() && albums.is_empty() && songs.is_empty() {
        println!("No search results");
        return;
    }

    if !artists.is_empty() {
        println!("Artists:");
        for artist in artists {
            println!(
                "  - {}",
                artist.get("name").and_then(|n| n.as_str()).unwrap_or("?")
            );
        }
    }

    if !albums.is_empty() {
        println!("Albums:");
        for album in albums {
            println!(
                "  - {} - {}",
                album.get("name").and_then(|n| n.as_str()).unwrap_or("?"),
                album
                    .get("artist")
                    .and_then(|a| a.as_str())
                    .unwrap_or("(unknown artist)")
            );
        }
    }

    if !songs.is_empty() {
        println!("Songs:");
        print_tracks(
            &songs
                .iter()
                .filter_map(|s| serde_json::from_value::<api::Track>((*s).clone()).ok())
                .collect::<Vec<api::Track>>(),
        );
    }
}

async fn run_cli() {
    let client = client_from_settings().await;

    let stats = client.get_server_stats().await;
    println!("Server stats: {stats:?}");

    let albums = client.get_all_albums(Some(10)).await;
    println!("Albums: {albums:?}");
}

async fn run_command(client: &api::NavidromeClient, command: &Command) {
    match command {
        Command::Stats => match client.get_server_stats().await {
            Ok(stats) => print_json(&stats),
            Err(e) => eprintln!("failed to fetch stats: {e}"),
        },
        Command::Albums { limit } => match client.get_all_albums(*limit).await {
            Ok(albums) => print_json(&albums),
            Err(e) => eprintln!("failed to fetch albums: {e}"),
        },
        Command::Tracks { limit } => match client.get_all_songs(*limit).await {
            Ok(tracks) => print_tracks(&tracks),
            Err(e) => eprintln!("failed to fetch tracks: {e}"),
        },
        Command::Artists { limit } => match client.get_all_artists(*limit).await {
            Ok(artists) => print_json(&artists),
            Err(e) => eprintln!("failed to fetch artists: {e}"),
        },
        Command::Search { query, limit } => match client.search(query, *limit).await {
            Ok(results) => print_search(&results),
            Err(e) => eprintln!("failed to search: {e}"),
        },
        Command::Sync { format, limit } => {
            let settings = match settings::load() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("failed to load settings: {e}");
                    return;
                }
            };
            run_sync(
                client,
                &settings.music_dir,
                format.as_deref(),
                settings.pref_format.as_deref(),
                *limit,
            )
            .await;
        }
        Command::Check => run_check(client).await,
    }
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

fn byte_size_display(bytes: i64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{bytes} bytes")
    }
}

fn resolve_format<'a>(
    format_arg: Option<&'a str>,
    pref_format: Option<&'a str>,
) -> Option<&'a str> {
    match format_arg {
        Some(f) => {
            eprintln!("warning: --format '{f}' overrides the format configured in settings.toml");
            Some(f)
        }
        None => pref_format,
    }
}

struct SyncResult {
    downloaded: usize,
    skipped: usize,
    failed: usize,
    synced: Vec<api::Track>,
}

async fn download_tracks(
    client: &api::NavidromeClient,
    tracks: &[api::Track],
    music_dir: &str,
    format: Option<&str>,
) -> SyncResult {
    let dir = std::path::PathBuf::from(music_dir);
    let mut downloaded = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;
    let mut synced: Vec<api::Track> = Vec::new();

    for (i, track) in tracks.iter().enumerate() {
        let title = track
            .title
            .clone()
            .unwrap_or_else(|| format!("track-{}", track.id.clone().unwrap_or_default()));
        let artist = track.artist.as_deref().unwrap_or("(unknown artist)");
        let album = track.album.as_deref().unwrap_or("(unknown album)");
        let extension = match format {
            Some(f) => f.to_lowercase(),
            None => track
                .suffix
                .clone()
                .unwrap_or_else(|| "bin".to_string())
                .to_lowercase(),
        };

        let subdir = dir.join(sanitize(artist)).join(sanitize(album));
        let target = subdir.join(format!("{}.{}", sanitize(&title), extension));

        if target.exists() {
            println!(
                "[{}/{}] already exists, skipping: {}",
                i + 1,
                tracks.len(),
                target.display()
            );
            skipped += 1;
            synced.push(track.clone());
            continue;
        }

        let Some(track_id) = track.id.clone() else {
            eprintln!("[{}/{}] missing id, skipping: {title}", i + 1, tracks.len());
            failed += 1;
            continue;
        };

        match client.download_track(&track_id, format).await {
            Ok(bytes) => {
                if let Err(e) = std::fs::create_dir_all(&subdir) {
                    eprintln!(
                        "[{}/{}] cannot create {}: {e}",
                        i + 1,
                        tracks.len(),
                        subdir.display()
                    );
                    failed += 1;
                    continue;
                }
                if let Err(e) = std::fs::write(&target, &bytes) {
                    eprintln!(
                        "[{}/{}] cannot write {}: {e}",
                        i + 1,
                        tracks.len(),
                        target.display()
                    );
                    failed += 1;
                    continue;
                }
                downloaded += 1;
                synced.push(track.clone());
                println!(
                    "[{}/{}] ok: {} ({})",
                    i + 1,
                    tracks.len(),
                    target.display(),
                    byte_size_display(bytes.len() as i64)
                );
            }
            Err(e) => {
                eprintln!("[{}/{}] failed ({title}): {e}", i + 1, tracks.len());
                failed += 1;
            }
        }
    }

    SyncResult {
        downloaded,
        skipped,
        failed,
        synced,
    }
}

async fn run_sync(
    client: &api::NavidromeClient,
    music_dir: &str,
    format_arg: Option<&str>,
    pref_format: Option<&str>,
    limit: Option<i64>,
) {
    let format = resolve_format(format_arg, pref_format);
    match format {
        Some(f) => println!("Download format: {f}"),
        None => eprintln!(
            "warning: no format requested (--format) and none set in settings.toml; \
             downloading original files"
        ),
    }

    println!("Fetching track list from the server...");
    let mut tracks = match client.list_all_tracks().await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("failed to list tracks: {e}");
            return;
        }
    };
    println!("Server reports {} track(s) total", tracks.len());

    let mut seen = std::collections::HashSet::new();
    tracks.retain(|t| {
        let id = t.id.clone().unwrap_or_default();
        seen.insert(id)
    });

    if let Some(limit) = limit {
        tracks.truncate(limit.max(0) as usize);
    }
    if tracks.is_empty() {
        println!("Nothing to download");
        return;
    }

    let total_bytes: i64 = tracks.iter().filter_map(|t| t.size).sum();
    println!(
        "Downloading {} unique track(s) (~{})",
        tracks.len(),
        byte_size_display(total_bytes)
    );

    let result = download_tracks(client, &tracks, music_dir, format).await;

    client.merge_synced_tracks(&result.synced);
    println!(
        "Synced-tracks cache updated with {} track(s)",
        result.synced.len()
    );

    println!(
        "Sync complete: {} downloaded, {} skipped, {} failed",
        result.downloaded, result.skipped, result.failed
    );
}

async fn run_check(client: &api::NavidromeClient) {
    println!("Fetching the current track list from the server...");
    let mut current = match client.list_all_tracks().await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("failed to fetch tracks from the server: {e}");
            return;
        }
    };
    let mut seen = std::collections::HashSet::new();
    current.retain(|t| {
        let id = t.id.clone().unwrap_or_default();
        seen.insert(id)
    });
    println!("Server reports {} track(s)", current.len());

    let current_ids: std::collections::HashSet<String> = current
        .iter()
        .map(|t| t.id.clone().unwrap_or_default())
        .collect();

    match client.cached_full_tracks() {
        Some(cached) => {
            let cached_ids: std::collections::HashSet<String> = cached
                .iter()
                .map(|t| t.id.clone().unwrap_or_default())
                .collect();
            let new_tracks: Vec<&api::Track> = current
                .iter()
                .filter(|t| {
                    let id = t.id.clone().unwrap_or_default();
                    !id.is_empty() && !cached_ids.contains(&id)
                })
                .collect();

            println!("Snapshot from last check: {} track(s)", cached.len());
            if new_tracks.is_empty() {
                println!("No new tracks.");
            } else {
                let owned: Vec<api::Track> = new_tracks.into_iter().cloned().collect();
                println!("Found {} new track(s):", owned.len());
                print_tracks(&owned);
            }
        }
        None => {
            println!("No previous snapshot found; storing this run as the baseline.");
        }
    }

    let synced = match client.cached_synced_tracks() {
        Some(synced) => Some(synced),
        None => {
            let music_dir = match settings::load().map(|s| s.music_dir) {
                Ok(dir) => dir,
                Err(e) => {
                    eprintln!("failed to load settings: {e}");
                    return;
                }
            };
            let dir = std::path::PathBuf::from(&music_dir);

            if !dir.is_dir() {
                println!();
                println!(
                    "No downloaded-tracks cache and no music directory ({music_dir}); run 'navisync sync' to download."
                );
                None
            } else {
                println!();
                println!(
                    "No synced-tracks cache yet; scanning {music_dir} for downloaded content..."
                );
                let on_disk = find_synced_on_disk(&dir, &current);
                if on_disk.is_empty() {
                    println!("No downloaded content found; nothing to cache.");
                    None
                } else {
                    client.merge_synced_tracks(&on_disk);
                    println!(
                        "Found {} downloaded track(s) on disk; synced-tracks cache generated.",
                        on_disk.len()
                    );
                    Some(on_disk)
                }
            }
        }
    };

    if let Some(synced) = synced {
        let synced_ids: std::collections::HashSet<String> = synced
            .iter()
            .map(|t| t.id.clone().unwrap_or_default())
            .collect();
        let not_synced: Vec<&api::Track> = current
            .iter()
            .filter(|t| {
                let id = t.id.clone().unwrap_or_default();
                !id.is_empty() && !synced_ids.contains(&id)
            })
            .collect();

        println!();
        println!(
            "Local library: {} track(s) recorded as synced",
            synced.len()
        );
        if not_synced.is_empty() {
            println!("Everything on the server is downloaded.");
        } else {
            println!(
                "{} track(s) on the server are NOT downloaded yet.",
                not_synced.len()
            );
            const MAX_SHOWN: usize = 10;
            for track in not_synced.iter().take(MAX_SHOWN) {
                println!(
                    "  - {} - {} [{album}]",
                    track.title.as_deref().unwrap_or("(unknown title)"),
                    track.artist.as_deref().unwrap_or("(unknown artist)"),
                    album = track.album.as_deref().unwrap_or("(unknown album)"),
                );
            }
            if not_synced.len() > MAX_SHOWN {
                println!("  ... and {} more", not_synced.len() - MAX_SHOWN);
            }
            println!("Run 'navisync sync' to download them.");
        }
    }

    client.store_full_tracks_cache(&current);
    println!("Snapshot saved to cache/tracks-all.json");
}

const SCAN_EXTENSIONS: [&str; 9] = [
    "mp3", "flac", "wav", "m4a", "aac", "ogg", "opus", "wma", "bin",
];

fn find_downloaded_file(dir: &std::path::Path, track: &api::Track) -> Option<std::path::PathBuf> {
    let title = track
        .title
        .clone()
        .unwrap_or_else(|| format!("track-{}", track.id.clone().unwrap_or_default()));
    let artist = track.artist.as_deref().unwrap_or("(unknown artist)");
    let album = track.album.as_deref().unwrap_or("(unknown album)");
    let subdir = dir.join(sanitize(artist)).join(sanitize(album));

    let mut extensions = Vec::new();
    if let Some(suffix) = &track.suffix {
        extensions.push(suffix.to_lowercase());
    }
    for ext in SCAN_EXTENSIONS {
        let ext = ext.to_string();
        if !extensions.contains(&ext) {
            extensions.push(ext);
        }
    }

    for extension in extensions {
        let candidate = subdir.join(format!("{}.{}", sanitize(&title), extension));
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn find_synced_on_disk(dir: &std::path::Path, server_tracks: &[api::Track]) -> Vec<api::Track> {
    server_tracks
        .iter()
        .filter(|track| {
            track
                .id
                .as_deref()
                .map(|id| !id.is_empty())
                .unwrap_or(false)
        })
        .filter(|track| find_downloaded_file(dir, track).is_some())
        .cloned()
        .collect()
}

async fn run_daemon() {
    let settings = match settings::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to load settings: {e}");
            return;
        }
    };

    let refresh_ms = settings.refresh.unwrap_or(60_000).max(0) as u64;
    let music_dir = settings.music_dir.clone();
    let pref_format = settings.pref_format.clone();

    println!("Daemon started; checking the server every {refresh_ms} ms (Ctrl+C to stop)");
    match pref_format.as_deref() {
        Some(f) => println!("Download format: {f}"),
        None => println!("Downloading original files (no format configured)"),
    }

    loop {
        daemon_cycle(&music_dir, pref_format.as_deref()).await;
        tokio::time::sleep(std::time::Duration::from_millis(refresh_ms)).await;
    }
}

async fn daemon_cycle(music_dir: &str, pref_format: Option<&str>) {
    let client = client_from_settings().await;

    let mut current = match client.list_all_tracks().await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("server check failed: {e} (retrying next cycle)");
            return;
        }
    };
    let mut seen = std::collections::HashSet::new();
    current.retain(|t| {
        let id = t.id.clone().unwrap_or_default();
        seen.insert(id)
    });
    println!("Server reports {} track(s)", current.len());

    let synced_ids: std::collections::HashSet<String> = match client.cached_synced_tracks() {
        Some(synced) => synced
            .iter()
            .map(|t| t.id.clone().unwrap_or_default())
            .collect(),
        None => {
            let dir = std::path::PathBuf::from(music_dir);
            let on_disk = find_synced_on_disk(&dir, &current);
            if !on_disk.is_empty() {
                client.merge_synced_tracks(&on_disk);
                println!(
                    "No synced-tracks cache; generated one from {} downloaded file(s)",
                    on_disk.len()
                );
            }
            on_disk
                .iter()
                .map(|t| t.id.clone().unwrap_or_default())
                .collect()
        }
    };

    let pending: Vec<api::Track> = current
        .iter()
        .filter(|t| {
            let id = t.id.clone().unwrap_or_default();
            !id.is_empty() && !synced_ids.contains(&id)
        })
        .cloned()
        .collect();

    if pending.is_empty() {
        println!("No new or missing tracks; local library up to date");
    } else {
        println!(
            "{} track(s) new or missing locally; syncing...",
            pending.len()
        );
        let result = download_tracks(&client, &pending, music_dir, pref_format).await;
        client.merge_synced_tracks(&result.synced);
        println!(
            "Cycle complete: {} downloaded, {} skipped, {} failed",
            result.downloaded, result.skipped, result.failed
        );
    }

    client.store_full_tracks_cache(&current);
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    if args.daemon {
        run_daemon().await
    } else if let Some(command) = &args.command {
        let client = client_from_settings().await;
        run_command(&client, command).await
    } else {
        run_cli().await
    }
}
