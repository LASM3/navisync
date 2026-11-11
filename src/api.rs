use base64::Engine;
use base64::engine::general_purpose::URL_SAFE;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

use crate::cache::Cache;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Track {
    pub id: Option<String>,
    pub title: Option<String>,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub size: Option<i64>,
    #[serde(default)]
    pub suffix: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NavidromeClient {
    base_url: String,
    username: String,
    password: String,
    token: Option<String>,
    cache: Option<Cache>,
    client: Client,
}

impl NavidromeClient {
    pub fn new_with_token(base_url: &str, username: &str, token: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            username: username.to_string(),
            password: String::new(),
            token: Some(token.to_string()),
            cache: Some(Cache::new(Cache::default_dir())),
            client: Client::new(),
        }
    }

    pub fn new(base_url: &str, username: &str, password: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            username: username.to_string(),
            password: password.to_string(),
            token: None,
            cache: Some(Cache::new(Cache::default_dir())),
            client: Client::new(),
        }
    }

    fn build_auth_query(&self) -> String {
        let mut params = format!("u={}&v=1.8.0&c=navisync&f=json", self.username);
        match &self.token {
            Some(t) if !t.is_empty() => {
                params.push_str(&format!("&jwt={t}"));
            }
            _ => {
                let encoded_pass: String =
                    form_urlencoded::byte_serialize(self.password.as_bytes()).collect();
                params.push_str(&format!("&p={encoded_pass}"));
            }
        }
        params
    }

    fn build_url(&self, path: &str, query_params: &str) -> String {
        let delimiter = if path.contains('?') { "&" } else { "?" };
        let auth_query = self.build_auth_query();
        let full_query = if query_params.is_empty() {
            format!("{delimiter}{auth_query}")
        } else {
            format!("{delimiter}{query_params}&{auth_query}")
        };
        format!("{}{path}{full_query}", self.base_url)
    }

    async fn get_bytes(&self, path: &str, query_params: &str) -> Result<Vec<u8>, String> {
        let url = self.build_url(path, query_params);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Request to {url} failed: {e:#}"))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Request to {url} failed: HTTP {status}: {body}"));
        }

        response
            .bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| format!("Failed to read response from {url}: {e}"))
    }

    async fn get(&self, path: &str, query_params: &str) -> Result<serde_json::Value, String> {
        let url = self.build_url(path, query_params);
        let body_bytes = self.get_bytes(path, query_params).await?;
        let body = String::from_utf8_lossy(&body_bytes).into_owned();

        let json: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| format!("Failed to parse JSON response from {url}: {e}"))?;

        let sub_resp = json
            .get("subsonic-response")
            .ok_or_else(|| "Response missing 'subsonic-response' root key".to_string())?;

        if sub_resp.get("status").and_then(|s| s.as_str()) == Some("failed") {
            let error_msg = sub_resp
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown Subsonic API error");
            return Err(format!("Subsonic API error: {error_msg}"));
        }

        Ok(sub_resp.clone())
    }

    async fn get_cached(
        &self,
        key: &str,
        path: &str,
        query_params: &str,
    ) -> Result<serde_json::Value, String> {
        if let Some(cache) = &self.cache {
            if let Some(body) = cache.load(key) {
                if let Ok(value) = serde_json::from_str(&body) {
                    return Ok(value);
                }
            }
        }

        let value = self.get(path, query_params).await?;

        if let Some(cache) = &self.cache {
            if let Ok(body) = serde_json::to_string(&value) {
                cache.save(key, &body);
            }
        }

        Ok(value)
    }

    pub async fn get_server_stats(&self) -> Result<serde_json::Value, String> {
        let query = format!("username={}", self.username);
        self.get("/rest/getUser", &query).await
    }

    pub async fn search(
        &self,
        query: &str,
        max_results: Option<i64>,
    ) -> Result<serde_json::Value, String> {
        let encoded_query: String = form_urlencoded::byte_serialize(query.as_bytes()).collect();
        let mut params = format!("query={encoded_query}");
        if let Some(n) = max_results {
            params.push_str(&format!("&artistCount={n}&albumCount={n}&songCount={n}"));
        }
        self.get("/rest/search2", &params).await
    }

    pub async fn get_all_albums(
        &self,
        max_records: Option<i64>,
    ) -> Result<serde_json::Value, String> {
        let query = match max_records {
            Some(n) => format!("type=alphabeticalByArtist&size={n}"),
            None => "type=alphabeticalByArtist".to_string(),
        };
        self.get_cached(
            &format!("albums-{max_records:?}"),
            "/rest/getAlbumList2",
            &query,
        )
        .await
    }

    pub async fn get_all_artists(
        &self,
        max_records: Option<i64>,
    ) -> Result<serde_json::Value, String> {
        self.get_cached(&format!("artists-{max_records:?}"), "/rest/getArtists", "")
            .await
    }

    pub async fn get_all_songs(&self, max_records: Option<i64>) -> Result<Vec<Track>, String> {
        let query = match max_records {
            Some(n) => format!("size={n}"),
            None => String::new(),
        };
        let response = self
            .get_cached(
                &format!("tracks-{max_records:?}"),
                "/rest/getRandomSongs",
                &query,
            )
            .await?;

        let empty_vec = Vec::new();
        let song_list = response
            .get("randomSongs")
            .and_then(|rs| rs.get("song"))
            .and_then(|s| s.as_array())
            .unwrap_or(&empty_vec);

        let tracks: Result<Vec<Track>, String> = song_list
            .iter()
            .map(|s| {
                serde_json::from_value::<Track>(s.clone())
                    .map_err(|e| format!("Failed to parse track entry: {e}"))
            })
            .collect();

        tracks
    }

    pub async fn get_all_tracks_size(&self) -> Result<i64, String> {
        let tracks = self.get_all_songs(None).await?;
        Ok(tracks.into_iter().filter_map(|t| t.size).sum())
    }

    pub const FULL_TRACKS_CACHE_KEY: &str = "tracks-all";

    pub fn cached_full_tracks(&self) -> Option<Vec<Track>> {
        let body = self.cache.as_ref()?.load(Self::FULL_TRACKS_CACHE_KEY)?;
        serde_json::from_str(&body).ok()
    }

    pub fn store_full_tracks_cache(&self, tracks: &[Track]) {
        let Ok(body) = serde_json::to_string(tracks) else {
            return;
        };
        if let Some(cache) = &self.cache {
            cache.save(Self::FULL_TRACKS_CACHE_KEY, &body);
        }
    }

    pub const SYNCED_CACHE_KEY: &str = "synced";

    pub fn cached_synced_tracks(&self) -> Option<Vec<Track>> {
        let body = self.cache.as_ref()?.load(Self::SYNCED_CACHE_KEY)?;
        serde_json::from_str(&body).ok()
    }

    pub fn merge_synced_tracks(&self, tracks: &[Track]) {
        let mut existing = self.cached_synced_tracks().unwrap_or_default();
        let mut ids: std::collections::HashSet<String> = existing
            .iter()
            .map(|t| t.id.clone().unwrap_or_default())
            .collect();

        for track in tracks {
            let id = track.id.clone().unwrap_or_default();
            if id.is_empty() || !ids.insert(id) {
                continue;
            }
            existing.push(track.clone());
        }

        if let Ok(body) = serde_json::to_string(&existing) {
            if let Some(cache) = &self.cache {
                cache.save(Self::SYNCED_CACHE_KEY, &body);
            }
        }
    }

    pub async fn list_all_tracks(&self) -> Result<Vec<Track>, String> {
        let mut tracks: Vec<Track> = Vec::new();
        let page_size = 500i64;
        let mut offset = 0i64;

        loop {
            let resp = self
                .get(
                    "/rest/getAlbumList2",
                    &format!("type=alphabeticalByArtist&size={page_size}&offset={offset}"),
                )
                .await?;
            let albums = resp
                .pointer("/albumList2/album")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            if albums.is_empty() {
                break;
            }

            for album in &albums {
                let Some(album_id) = album.get("id").and_then(|i| i.as_str()) else {
                    continue;
                };
                let album_resp = match self.get("/rest/getAlbum", &format!("id={album_id}")).await {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("failed to fetch album {album_id}: {e}");
                        continue;
                    }
                };
                if let Some(songs) = album_resp.pointer("/album/song").and_then(|v| v.as_array()) {
                    for song in songs {
                        match serde_json::from_value::<Track>(song.clone()) {
                            Ok(track) => tracks.push(track),
                            Err(e) => eprintln!("failed to parse track entry: {e}"),
                        }
                    }
                }
            }

            offset += page_size;
            if (albums.len() as i64) < page_size {
                break;
            }
        }

        Ok(tracks)
    }

    pub async fn download_track(
        &self,
        track_id: &str,
        format: Option<&str>,
    ) -> Result<Vec<u8>, String> {
        let mut params = format!(
            "id={}",
            form_urlencoded::byte_serialize(track_id.as_bytes()).collect::<String>()
        );
        if let Some(format) = format {
            params.push_str(&format!("&format={}", format.to_lowercase()));
        }
        let endpoint = if format.is_some() {
            "/rest/stream"
        } else {
            "/rest/download"
        };
        self.get_bytes(endpoint, &params).await
    }
}

pub async fn login(base_url: &str, username: &str, password: &str) -> Result<String, String> {
    let url = format!("{}/auth/login", base_url.trim_end_matches('/'));

    let client = Client::new();
    let response = client
        .post(&url)
        .json(&serde_json::json!({
            "username": username,
            "password": password,
        }))
        .send()
        .await
        .map_err(|e| format!("login request to {url} failed: {e:#}"))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("failed to read login response from {url}: {e}"))?;

    if !status.is_success() {
        return Err(format!("login failed: HTTP {status}: {body}"));
    }

    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("failed to parse login response from {url}: {e}"))?;

    json.get("token")
        .and_then(|t| t.as_str())
        .map(|t| t.to_string())
        .ok_or_else(|| format!("login response missing token: {body}"))
}

pub fn token_expired(token: &str) -> bool {
    let Some(payload) = token.split('.').nth(1) else {
        return false;
    };

    let Ok(decoded) = URL_SAFE.decode(payload) else {
        return false;
    };
    let Ok(claims) = serde_json::from_slice::<serde_json::Value>(&decoded) else {
        return false;
    };

    let Some(exp) = claims.get("exp").and_then(|e| e.as_i64()) else {
        return false;
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    now >= exp
}
