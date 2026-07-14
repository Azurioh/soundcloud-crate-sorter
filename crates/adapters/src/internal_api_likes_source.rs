//! `InternalApiLikesSource` — reads public likes via the unofficial `api-v2.soundcloud.com`
//! endpoints (research R1). No login. Handles `client_id` extraction (it rotates), `next_href`
//! cursor pagination, and rate-limit backoff/retry internally.

use std::sync::Mutex;
use std::time::Duration;

use application::ports::likes_source::{LikesSourceError, LikesSourcePort, SourceUserId};
use async_trait::async_trait;
use domain::track::LikedTrack;
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use serde::Deserialize;
use tokio::time::sleep;

/// Base URL of the unofficial api-v2.
const API_BASE: &str = "https://api-v2.soundcloud.com";
/// The web player host, used to extract a fresh `client_id`.
const WEB_BASE: &str = "https://soundcloud.com";
/// Page size for the likes endpoint.
const LIKES_LIMIT: u32 = 200;
/// Hard cap on likes pages to follow. `next_href` comes from an unofficial endpoint; a
/// non-terminating or cyclic cursor must not loop forever / grow the result unboundedly. At
/// `LIKES_LIMIT` per page this bounds a single sync well above any real library.
const MAX_LIKES_PAGES: u32 = 500;
/// Maximum retry attempts on a rate-limited (429) response.
const MAX_RETRIES: u32 = 3;
/// Base backoff between retries (multiplied by the attempt number).
const BASE_BACKOFF_MS: u64 = 1_000;
/// Marker preceding a `client_id` in the web player's bundled JS.
const CLIENT_ID_MARKER: &str = "client_id:\"";

/// Reads public likes from the SoundCloud api-v2.
pub struct InternalApiLikesSource {
    client: Client,
    client_id: Mutex<Option<String>>,
}

impl InternalApiLikesSource {
    /// Builds the source, resolving a `client_id` lazily on first use.
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            client_id: Mutex::new(None),
        }
    }

    /// Builds the source with a pre-supplied `client_id` (bypasses extraction).
    #[must_use]
    pub fn with_client_id(client_id: String) -> Self {
        Self {
            client: Client::new(),
            client_id: Mutex::new(Some(client_id)),
        }
    }

    /// Returns a usable `client_id`, extracting and caching one if needed.
    async fn client_id(&self) -> Result<String, LikesSourceError> {
        if let Some(id) = self.cached_client_id() {
            return Ok(id);
        }
        let extracted = self.extract_client_id().await?;
        *self.client_id.lock().expect("client_id mutex poisoned") = Some(extracted.clone());
        Ok(extracted)
    }

    /// Returns the cached `client_id`, if any (lock is not held across an await).
    fn cached_client_id(&self) -> Option<String> {
        self.client_id
            .lock()
            .expect("client_id mutex poisoned")
            .clone()
    }

    /// Extracts a `client_id` from the web player's bundled JavaScript (research R1).
    async fn extract_client_id(&self) -> Result<String, LikesSourceError> {
        let html = self
            .client
            .get(WEB_BASE)
            .send()
            .await
            .map_err(transport)?
            .text()
            .await
            .map_err(transport)?;

        // Script bundles are last in the document; scan them newest-first for the id.
        let mut script_urls = extract_script_urls(&html);
        script_urls.reverse();
        for url in script_urls {
            let script = self.client.get(&url).send().await.map_err(transport)?;
            let body = script.text().await.map_err(transport)?;
            if let Some(id) = extract_client_id_from_script(&body) {
                return Ok(id);
            }
        }
        Err(LikesSourceError::Transport {
            source: "could not extract client_id".into(),
        })
    }

    /// Sends a request, retrying on 429 with linear backoff (research R1).
    async fn send_with_retry(&self, request: RequestBuilder) -> Result<Response, LikesSourceError> {
        for attempt in 0..=MAX_RETRIES {
            let attempt_request =
                request
                    .try_clone()
                    .ok_or_else(|| LikesSourceError::Transport {
                        source: "request is not clonable for retry".into(),
                    })?;
            let response = attempt_request.send().await.map_err(transport)?;
            if response.status() != StatusCode::TOO_MANY_REQUESTS || attempt == MAX_RETRIES {
                return Ok(response);
            }
            sleep(Duration::from_millis(
                BASE_BACKOFF_MS * u64::from(attempt + 1),
            ))
            .await;
        }
        // The loop always returns on the final attempt; this is a defensive fallback.
        Err(LikesSourceError::RateLimited)
    }
}

impl Default for InternalApiLikesSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LikesSourcePort for InternalApiLikesSource {
    async fn resolve_user(&self, profile_url: &str) -> Result<SourceUserId, LikesSourceError> {
        let client_id = self.client_id().await?;
        let request = self
            .client
            .get(format!("{API_BASE}/resolve"))
            .query(&[("url", profile_url), ("client_id", &client_id)]);
        let response = self.send_with_retry(request).await?;

        match response.status() {
            StatusCode::NOT_FOUND => Err(LikesSourceError::ProfileNotFound),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                Err(LikesSourceError::ProfilePrivate)
            }
            StatusCode::TOO_MANY_REQUESTS => Err(LikesSourceError::RateLimited),
            status if status.is_success() => {
                let resolved: ResolveResponse = response.json().await.map_err(transport)?;
                Ok(SourceUserId::new(resolved.id.to_string()))
            }
            _ => Err(LikesSourceError::Transport {
                source: format!("resolve failed: HTTP {}", response.status()).into(),
            }),
        }
    }

    async fn list_likes(&self, user: &SourceUserId) -> Result<Vec<LikedTrack>, LikesSourceError> {
        let client_id = self.client_id().await?;
        let first_url = format!(
            "{API_BASE}/users/{}/likes/tracks?client_id={}&limit={}&linked_partitioning=1",
            user.as_str(),
            client_id,
            LIKES_LIMIT
        );

        let mut liked = Vec::new();
        let mut next_url = Some(first_url);
        for _ in 0..MAX_LIKES_PAGES {
            let Some(url) = next_url.take() else {
                return Ok(liked);
            };
            let response = self.send_with_retry(self.client.get(&url)).await?;
            match response.status() {
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                    return Err(LikesSourceError::ProfilePrivate)
                }
                StatusCode::TOO_MANY_REQUESTS => return Err(LikesSourceError::RateLimited),
                status if !status.is_success() => {
                    return Err(LikesSourceError::Transport {
                        source: format!("likes page failed: HTTP {status}").into(),
                    })
                }
                _ => {}
            }
            let page: LikesPage = response.json().await.map_err(transport)?;
            for track in page.collection {
                liked.push(track_to_liked(track));
            }
            next_url = page.next_href;
        }
        // The cursor never terminated within the page cap — treat as a misbehaving endpoint
        // rather than looping forever.
        Err(LikesSourceError::Transport {
            source: format!("likes pagination exceeded {MAX_LIKES_PAGES} pages").into(),
        })
    }
}

/// Extracts `https://…sndcdn.com/assets/*.js` script URLs from the player HTML.
fn extract_script_urls(html: &str) -> Vec<String> {
    let mut urls = Vec::new();
    for fragment in html.split("src=\"").skip(1) {
        if let Some(end) = fragment.find('"') {
            let url = &fragment[..end];
            if url.ends_with(".js") && url.contains("sndcdn.com") {
                urls.push(url.to_owned());
            }
        }
    }
    urls
}

/// Finds a `client_id:"…"` value inside a bundled script.
fn extract_client_id_from_script(script: &str) -> Option<String> {
    let start = script.find(CLIENT_ID_MARKER)? + CLIENT_ID_MARKER.len();
    let rest = &script[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

/// Maps a SoundCloud track object onto the vendor-free `LikedTrack`.
fn track_to_liked(track: TrackDto) -> LikedTrack {
    LikedTrack {
        source_track_id: track.id.to_string(),
        title: track.title.unwrap_or_default(),
        artist: track.user.map(|u| u.username).unwrap_or_default(),
        source_genre: track.genre,
        duration_ms: track.duration,
        permalink_url: track.permalink_url.unwrap_or_default(),
        artwork_url: track.artwork_url,
    }
}

/// Wraps a transport-level error, never leaking secrets (the key/id live in headers/query, not here).
fn transport<E: std::error::Error + Send + Sync + 'static>(error: E) -> LikesSourceError {
    LikesSourceError::Transport {
        source: Box::new(error),
    }
}

/// `/resolve` response — only the numeric user id is consumed.
#[derive(Deserialize)]
struct ResolveResponse {
    id: u64,
}

/// One page of the likes endpoint.
#[derive(Deserialize)]
struct LikesPage {
    #[serde(default)]
    collection: Vec<TrackDto>,
    #[serde(default)]
    next_href: Option<String>,
}

/// A SoundCloud track object (only the fields we consume; schema is validated at build time, R1).
#[derive(Deserialize)]
struct TrackDto {
    id: u64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    genre: Option<String>,
    #[serde(default)]
    duration: u64,
    #[serde(default)]
    permalink_url: Option<String>,
    #[serde(default)]
    artwork_url: Option<String>,
    #[serde(default)]
    user: Option<UserDto>,
}

/// The uploader object.
#[derive(Deserialize)]
struct UserDto {
    #[serde(default)]
    username: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_script_urls_ending_in_js() {
        let html = r#"<script src="https://a-v2.sndcdn.com/assets/0-abc.js"></script>
                      <script src="https://example.com/other.css"></script>
                      <script src="https://a-v2.sndcdn.com/assets/9-def.js"></script>"#;
        let urls = extract_script_urls(html);
        assert_eq!(urls.len(), 2);
        assert!(urls[0].ends_with("0-abc.js"));
    }

    #[test]
    fn extracts_client_id_from_script_body() {
        let script = r#"…,client_id:"AbC123xyz",app_version…"#;
        assert_eq!(
            extract_client_id_from_script(script).as_deref(),
            Some("AbC123xyz")
        );
    }

    #[test]
    fn maps_track_dto_to_liked() {
        let dto = TrackDto {
            id: 42,
            title: Some("Night Drive".into()),
            genre: Some("House".into()),
            duration: 300_000,
            permalink_url: Some("https://sc/x".into()),
            artwork_url: None,
            user: Some(UserDto {
                username: "artist".into(),
            }),
        };
        let liked = track_to_liked(dto);
        assert_eq!(liked.source_track_id, "42");
        assert_eq!(liked.artist, "artist");
        assert_eq!(liked.source_genre.as_deref(), Some("House"));
    }
}
