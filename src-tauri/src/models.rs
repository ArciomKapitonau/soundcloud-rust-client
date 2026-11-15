//! Data models shared between the backend modules and the frontend.
//!
//! The official API (api.soundcloud.com) and the private web API
//! (api-v2.soundcloud.com) return slightly different JSON shapes; everything
//! is normalized into the structs below.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: u64,
    pub title: String,
    pub artist: String,
    pub artist_id: Option<u64>,
    pub artist_avatar: Option<String>,
    pub artwork_url: Option<String>,
    pub duration_ms: u64,
    pub waveform_url: Option<String>,
    pub permalink_url: Option<String>,
    pub genre: Option<String>,
    pub playback_count: Option<u64>,
    pub likes_count: Option<u64>,
    pub created_at: Option<String>,
    /// api-v2 only: list of available transcodings.
    #[serde(default)]
    pub media: Option<Value>,
    /// api-v2 only: required to resolve stream URLs.
    #[serde(default)]
    pub track_authorization: Option<String>,
    /// Some tracks are only available as a 30 second preview (Go+).
    #[serde(default)]
    pub policy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub system_id: Option<String>,
    pub is_album: bool,
    pub id: u64,
    pub title: String,
    pub artist: String,
    pub artwork_url: Option<String>,
    pub track_count: u64,
    pub duration_ms: u64,
    pub permalink_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: u64,
    pub username: String,
    pub avatar_url: Option<String>,
    pub permalink_url: Option<String>,
    pub followers_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub tracks: Vec<Track>,
    pub next_href: Option<String>,
}

fn str_of(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn u64_of(v: &Value, key: &str) -> Option<u64> {
    v.get(key).and_then(|x| x.as_u64().or_else(|| x.as_f64().map(|f| f as u64)))
}

/// SoundCloud serves `-large` (100x100) artwork by default; ask for 500x500.
pub fn hi_res_artwork(url: Option<String>) -> Option<String> {
    url.map(|u| u.replace("-large.", "-t500x500."))
}

impl Track {
    /// Normalize a track from either API. Accepts the bare track object or the
    /// wrappers used in collections (`{track: ..}`, `{origin: ..}`).
    pub fn from_value(v: &Value) -> Option<Track> {
        if let Some(inner) = v.get("track").filter(|t| t.is_object()) {
            return Track::from_value(inner);
        }
        if let Some(inner) = v.get("origin").filter(|t| t.is_object()) {
            return Track::from_value(inner);
        }
        if v.get("playlist").is_some() {
            return None;
        }
        if let Some(kind) = v.get("kind").and_then(Value::as_str) {
            if kind != "track" {
                return None;
            }
        }
        let id = u64_of(v, "id")?;
        let user = v.get("user").cloned().unwrap_or(Value::Null);
        let title = str_of(v, "title")?;
        let artist_avatar = hi_res_artwork(str_of(&user, "avatar_url"));
        let artwork_url = hi_res_artwork(str_of(v, "artwork_url")).or_else(|| artist_avatar.clone());
        Some(Track {
            id,
            title,
            artist: str_of(&user, "username").unwrap_or_else(|| "Unknown artist".into()),
            artist_id: u64_of(&user, "id"),
            artist_avatar,
            artwork_url,
            duration_ms: u64_of(v, "full_duration")
                .or_else(|| u64_of(v, "duration"))
                .unwrap_or(0),
            waveform_url: str_of(v, "waveform_url"),
            permalink_url: str_of(v, "permalink_url"),
            genre: str_of(v, "genre"),
            playback_count: u64_of(v, "playback_count"),
            likes_count: u64_of(v, "likes_count").or_else(|| u64_of(v, "favoritings_count")),
            created_at: str_of(v, "created_at"),
            media: v.get("media").cloned(),
            track_authorization: str_of(v, "track_authorization"),
            policy: str_of(v, "policy"),
        })
    }

    /// Tracks embedded in some v2 collections are "stubs" with only an id.
    pub fn is_stub(v: &Value) -> bool {
        v.get("title").is_none() && v.get("id").is_some()
    }
}

impl Playlist {
    pub fn from_value(v: &Value) -> Option<Playlist> {
        if let Some(inner) = v.get("playlist").filter(|t| t.is_object()) {
            return Playlist::from_value(inner);
        }
        let system_id = (v.get("kind").and_then(Value::as_str) == Some("system-playlist"))
            .then(|| str_of(v, "id")).flatten();
        let id = u64_of(v, "id").or(system_id.as_ref().map(|_| 0))?;
        let user = v.get("user").cloned().unwrap_or(Value::Null);
        let first_track_art = v
            .get("tracks")
            .and_then(Value::as_array)
            .and_then(|a| a.iter().find_map(|t| str_of(t, "artwork_url")));
        Some(Playlist {
            system_id,
            is_album: v.get("is_album").and_then(Value::as_bool).unwrap_or(false) || matches!(v.get("set_type").and_then(Value::as_str), Some("album" | "ep" | "compilation")),
            id,
            title: str_of(v, "title").unwrap_or_else(|| "Без названия".into()),
            artist: str_of(&user, "username").unwrap_or_default(),
            artwork_url: hi_res_artwork(str_of(v, "artwork_url").or_else(|| str_of(v, "calculated_artwork_url")).or(first_track_art))
                .or_else(|| hi_res_artwork(str_of(&user, "avatar_url"))),
            track_count: u64_of(v, "track_count").unwrap_or_else(|| v.get("tracks").and_then(Value::as_array).map_or(0, |a| a.len() as u64)),
            duration_ms: u64_of(v, "duration").unwrap_or(0),
            permalink_url: str_of(v, "permalink_url"),
        })
    }
}

impl User {
    pub fn from_value(v: &Value) -> Option<User> {
        Some(User {
            id: u64_of(v, "id")?,
            username: str_of(v, "username").unwrap_or_default(),
            avatar_url: hi_res_artwork(str_of(v, "avatar_url")),
            permalink_url: str_of(v, "permalink_url"),
            followers_count: u64_of(v, "followers_count"),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverySection {
    pub title: String,
    pub tracks: Vec<Track>,
    pub playlists: Vec<Playlist>,
}

#[cfg(test)]
mod release_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn albums_and_system_collections_keep_their_identity() {
        let album = Playlist::from_value(&json!({"id":12,"set_type":"album","title":"Album"})).unwrap();
        assert!(album.is_album);
        let mix = Playlist::from_value(&json!({"id":"soundcloud:system-playlists:mix:123", "kind":"system-playlist", "calculated_artwork_url":"https://i1.sndcdn.com/a-large.jpg", "tracks":[{"id":1}]})).unwrap();
        assert_eq!(mix.system_id.as_deref(), Some("soundcloud:system-playlists:mix:123"));
        assert_eq!(mix.track_count, 1);
        assert!(mix.artwork_url.unwrap().ends_with("a-t500x500.jpg"));
        assert!(!mix.is_album);
    }
}
