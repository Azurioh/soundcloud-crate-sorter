//! `InternalApiLikesSource` — reads public likes via the unofficial `api-v2.soundcloud.com`
//! endpoints (research R1). No login. Handles `client_id` extraction (it rotates), `next_href`
//! cursor pagination, and rate-limit backoff/retry internally.

use std::sync::Mutex;
use std::time::Duration;

use application::ports::likes_source::{LikesSourceError, LikesSourcePort, SourceUserId};
use async_trait::async_trait;
use domain::track::LikedTrack;
use reqwest::{Client, RequestBuilder, Response, StatusCode, Url};
use serde::Deserialize;
use tokio::time::sleep;

/// Base URL of the unofficial api-v2.
const API_BASE: &str = "https://api-v2.soundcloud.com";
/// The web player host, used to extract a fresh `client_id`.
const WEB_BASE: &str = "https://soundcloud.com";
/// Path segment of the likes endpoint, under `/users/{id}/`.
///
/// **Not** `likes/tracks`, which is what research.md R1 recorded and this adapter originally shipped:
/// that path does not exist and 404s for every user, so scanning could never work. The web client
/// calls `track_likes`, which also has the virtue of returning only tracks — `likes` exists too but
/// mixes in playlist likes.
const LIKES_PATH: &str = "track_likes";
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
/// Total per-request budget. reqwest's async client has NO timeout by default: a peer that
/// completes TLS and then stalls would hang a scan forever with no error ever surfaced.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Connection-establishment budget, kept well under `REQUEST_TIMEOUT`.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Builds the HTTP client used for every api-v2 / web-player call.
fn build_client() -> Client {
    Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .expect("HTTP client builds from static timeouts")
}

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
            client: build_client(),
            client_id: Mutex::new(None),
        }
    }

    /// Builds the source with a pre-supplied `client_id` (bypasses extraction).
    #[must_use]
    pub fn with_client_id(client_id: String) -> Self {
        Self {
            client: build_client(),
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
        // A single unreachable bundle must not abort the scan — bundles rotate, and an older one
        // in the list may still carry a usable id — so failures skip to the next candidate and
        // only an exhausted list is an error.
        let mut script_urls = extract_script_urls(&html);
        script_urls.reverse();
        for url in script_urls {
            let Ok(script) = self.client.get(&url).send().await else {
                continue;
            };
            let Ok(body) = script.text().await else {
                continue;
            };
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
        let first_url = first_likes_url(user, &client_id)?;

        let mut liked = Vec::new();
        let mut next_url = Some(first_url);
        for _ in 0..MAX_LIKES_PAGES {
            let Some(url) = next_url.take() else {
                return Ok(liked);
            };
            // The cursor arrives without the id (see `with_client_id`), so re-attach it every page.
            let url = with_client_id(&url, &client_id)?;
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
            for like in page.collection {
                liked.push(track_to_liked(like.track));
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

/// Builds the first likes-page URL for `user`.
///
/// Built via `parse_with_params`, not `format!`: an id carrying `#` would truncate the query
/// (dropping `limit`/`linked_partitioning`, silently returning a short first page with no cursor)
/// and an `&` would inject a parameter. Extracted from `list_likes` so the endpoint it targets is
/// assertable without a network round-trip — the shipped path 404'd for months precisely because
/// nothing offline could see it.
fn first_likes_url(user: &SourceUserId, client_id: &str) -> Result<String, LikesSourceError> {
    Ok(Url::parse_with_params(
        &format!("{API_BASE}/users/{}/{LIKES_PATH}", user.as_str()),
        &[
            ("client_id", client_id),
            ("limit", &LIKES_LIMIT.to_string()),
            ("linked_partitioning", "1"),
        ],
    )
    .map_err(transport)?
    .to_string())
}

/// Ensures `url` carries a `client_id`, adding one if the API left it out.
///
/// The `next_href` cursor comes back with `offset` and `limit` but no `client_id`, so following it
/// as given is an anonymous request that 401s. Every page after the first has to have the id put
/// back. Existing pairs are left untouched — the cursor's `offset` is an opaque, percent-encoded
/// token and re-encoding it would invalidate the very thing it exists to carry.
fn with_client_id(url: &str, client_id: &str) -> Result<String, LikesSourceError> {
    let mut parsed = Url::parse(url).map_err(transport)?;
    if parsed.query_pairs().any(|(key, _)| key == "client_id") {
        return Ok(parsed.to_string());
    }
    parsed.query_pairs_mut().append_pair("client_id", client_id);
    Ok(parsed.to_string())
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
///
/// The marker is a naive scrape of third-party JS, so the captured span is validated before it is
/// ever trusted as a query value: a real id is an opaque alphanumeric token, and anything else
/// means the marker matched something that is not a client id.
fn extract_client_id_from_script(script: &str) -> Option<String> {
    let start = script.find(CLIENT_ID_MARKER)? + CLIENT_ID_MARKER.len();
    let rest = &script[start..];
    let end = rest.find('"')?;
    let candidate = &rest[..end];
    if candidate.is_empty() || !candidate.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(candidate.to_owned())
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
    collection: Vec<LikeDto>,
    #[serde(default)]
    next_href: Option<String>,
}

/// One entry in a likes page: the endpoint returns **likes, not tracks**, so the track we want is
/// nested under `track` alongside the like's own metadata (`created_at`, `kind`).
///
/// `track` is deliberately required. A missing one means the payload is not the shape we understand,
/// and reading a like whose track we cannot see would mean inventing the track — the page failing
/// loudly is the honest outcome (Principle I). Verified against a live 127-like library: every entry
/// carries its track.
#[derive(Deserialize)]
struct LikeDto {
    track: TrackDto,
}

/// A SoundCloud track object (only the fields we consume; schema is validated at build time, R1).
#[derive(Deserialize)]
struct TrackDto {
    id: u64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    genre: Option<String>,
    // Deliberately NOT `#[serde(default)]`: a defaulted 0 reads as "shorter than any mixable
    // track" to the FR-030 duration heuristic, which auto-files the track into the review crate
    // at high confidence. An absent duration must surface as a parse error, never a fabricated 0.
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

    /// A scraped span that is not an opaque alphanumeric token is not a client id. Accepting one
    /// would put `#` or `&` into the likes query string, truncating or injecting parameters.
    #[test]
    fn rejects_client_id_containing_url_metacharacters() {
        let script = r#"…,client_id:"abc#x",app_version…"#;
        assert_eq!(extract_client_id_from_script(script), None);
    }

    /// A track object without `duration` must fail to parse. Defaulting it to 0 made the FR-030
    /// heuristic read the track as non-music and auto-file it into the review crate at 0.95 —
    /// a silent misfile sourced entirely from an absent field.
    #[test]
    fn track_without_duration_is_a_parse_error_not_a_zero() {
        let json = r#"{"id":1,"title":"T","permalink_url":"u"}"#;
        assert!(serde_json::from_str::<TrackDto>(json).is_err());

        let with_duration = r#"{"id":1,"title":"T","permalink_url":"u","duration":180000}"#;
        let track: TrackDto = serde_json::from_str(with_duration).expect("valid track parses");
        assert_eq!(track_to_liked(track).duration_ms, 180_000);
    }

    /// A real page from the likes endpoint, trimmed to the fields we consume. Captured live rather
    /// than hand-imagined: the shape this adapter *assumed* parsed perfectly in every unit test and
    /// still failed against the real endpoint, so the fixture has to come from the wire.
    const REAL_TRACK_LIKES_PAGE: &str = r#"{
      "collection": [
        {
          "created_at": "2026-07-14T10:27:14.922Z",
          "kind": "like",
          "track": {
            "id": 2153425884,
            "title": "Pupa Nas T x FOVOS - Work (Edit)",
            "genre": "Trance",
            "duration": 259102,
            "permalink_url": "https://soundcloud.com/fovosmusic/fovos-work-edit",
            "artwork_url": "https://i1.sndcdn.com/artworks-gD08h1LYlAvlE95t.jpg",
            "user": { "username": "FOVOS" }
          }
        }
      ],
      "next_href": "https://api-v2.soundcloud.com/users/728093053/track_likes?offset=2026-07-14"
    }"#;

    /// The endpoint returns **likes, not tracks**: every collection item is a `{created_at, kind,
    /// track}` wrapper. Parsing the collection as bare `TrackDto` fails on every real page, which
    /// surfaced as an opaque "transport error reading SoundCloud likes" and made scanning impossible.
    #[test]
    fn parses_a_real_likes_page_and_unwraps_the_nested_track() {
        let page: LikesPage =
            serde_json::from_str(REAL_TRACK_LIKES_PAGE).expect("a real likes page parses");

        assert_eq!(page.collection.len(), 1);
        let liked = track_to_liked(page.collection.into_iter().next().expect("one like").track);
        assert_eq!(liked.source_track_id, "2153425884");
        assert_eq!(liked.title, "Pupa Nas T x FOVOS - Work (Edit)");
        assert_eq!(liked.artist, "FOVOS");
        assert_eq!(liked.source_genre.as_deref(), Some("Trance"));
        assert_eq!(liked.duration_ms, 259_102);
        assert!(page.next_href.is_some(), "the cursor must survive parsing");
    }

    /// `/users/{id}/likes/tracks` — the path research R1 recorded and this adapter shipped — does not
    /// exist: it 404s for every user. The live endpoint is `/users/{id}/track_likes`.
    #[test]
    fn first_page_url_targets_the_endpoint_that_actually_exists() {
        let url = first_likes_url(&SourceUserId::new("728093053".to_owned()), "cid123")
            .expect("url builds");

        assert!(
            url.starts_with("https://api-v2.soundcloud.com/users/728093053/track_likes?"),
            "unexpected likes URL: {url}"
        );
        assert!(url.contains("client_id=cid123"));
        assert!(url.contains("linked_partitioning=1"));
        assert!(url.contains("limit=200"));
    }

    /// The cursor the API hands back carries `offset` and `limit` but **not** `client_id`, so
    /// following it verbatim is an unauthenticated request: it 401s, which this adapter reads as
    /// "profile is private". Page one succeeds, page two kills the scan — and because SoundCloud
    /// emits a cursor even when the first page already returned everything, that happened to every
    /// library, however small.
    #[test]
    fn a_cursor_regains_the_client_id_the_api_leaves_out() {
        let next = "https://api-v2.soundcloud.com/users/1/track_likes?offset=2020-09-03T05%3A43%3A10.964Z%2Cuser-track-likes&limit=200";

        let url = with_client_id(next, "cid123").expect("url builds");

        assert!(url.contains("client_id=cid123"), "unauthenticated: {url}");
        assert!(
            url.contains("offset=2020-09-03T05%3A43%3A10.964Z%2Cuser-track-likes"),
            "the cursor's offset must survive re-encoding intact: {url}"
        );
        assert!(url.contains("limit=200"));
    }

    /// If the API ever starts including the id, we must not append a second one.
    #[test]
    fn a_cursor_that_already_carries_a_client_id_is_left_alone() {
        let next = "https://api-v2.soundcloud.com/users/1/track_likes?client_id=existing&limit=200";

        let url = with_client_id(next, "cid123").expect("url builds");

        assert!(url.contains("client_id=existing"));
        assert!(
            !url.contains("cid123"),
            "appended a duplicate client_id: {url}"
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
