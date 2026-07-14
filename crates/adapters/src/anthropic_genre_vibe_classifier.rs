//! `AnthropicGenreVibeClassifier` — genre/vibe classification via the Claude Messages API over HTTP
//! (research R8). Uses Haiku (cheap, high-volume). NEVER returns BPM/key/energy (Principle I).
//!
//! The API key is read from configuration and is never logged or placed in error messages
//! (error-design: no secrets on the wire). A malformed model response is mapped to
//! `ClassifyError::BadResponse` so the use case treats it as low confidence rather than aborting.

use application::ports::genre_vibe_classifier::{
    ClassificationInput, ClassifyError, GenreCandidate, GenreVibeClassifierPort,
    GenreVibeSuggestion,
};
use async_trait::async_trait;
use domain::confidence::Confidence;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

/// The Claude Messages API endpoint.
const MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";
/// Pinned API version header value.
const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Default model — cheap and fast, appropriate for light genre/vibe classification (research R8).
const DEFAULT_MODEL: &str = "claude-haiku-4-5-20251001";
/// The classification answer is a small JSON object; cap output tokens tightly.
const MAX_TOKENS: u32 = 512;
/// Lower bound used when clamping a model-supplied confidence into range.
const CONFIDENCE_MIN: f32 = 0.0;
/// Upper bound used when clamping a model-supplied confidence into range.
const CONFIDENCE_MAX: f32 = 1.0;
/// Max characters of any untrusted metadata field spliced into the prompt. Title/artist/genre/
/// description originate from attacker-controllable SoundCloud uploads; capping them bounds the
/// prompt-injection surface and the input-token cost/latency amplification of a huge description.
const MAX_PROMPT_FIELD_CHARS: usize = 400;

/// The system prompt: constrain the model to genre/vibe only, JSON output, no audio features.
const SYSTEM_PROMPT: &str =
    "You are a music genre and vibe classifier for a DJ crate-sorting tool. \
Given a track's metadata, return ONLY a JSON object of the form \
{\"candidates\":[{\"genre\":\"<genre>\",\"confidence\":<0.0-1.0>}],\"vibe_tags\":[\"<tag>\"]}. \
List candidate genres most-confident first. Never include BPM, key, tempo, or energy. \
Return only the JSON object, no prose.";

/// Claude Messages API classifier.
pub struct AnthropicGenreVibeClassifier {
    client: Client,
    api_key: String,
    model: String,
}

impl AnthropicGenreVibeClassifier {
    /// Builds the classifier with the default model.
    #[must_use]
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model: DEFAULT_MODEL.to_owned(),
        }
    }
}

#[async_trait]
impl GenreVibeClassifierPort for AnthropicGenreVibeClassifier {
    async fn classify(
        &self,
        input: &ClassificationInput,
    ) -> Result<GenreVibeSuggestion, ClassifyError> {
        let request = MessagesRequest {
            model: &self.model,
            max_tokens: MAX_TOKENS,
            system: SYSTEM_PROMPT,
            messages: vec![Message {
                role: "user",
                content: user_prompt(input),
            }],
        };

        let response = self
            .client
            .post(MESSAGES_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| ClassifyError::Transport {
                source: Box::new(e),
            })?;

        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            return Err(ClassifyError::RateLimited);
        }
        if !response.status().is_success() {
            return Err(ClassifyError::BadResponse {
                source: format!("classifier HTTP {}", response.status()).into(),
            });
        }

        let body: MessagesResponse =
            response
                .json()
                .await
                .map_err(|e| ClassifyError::BadResponse {
                    source: Box::new(e),
                })?;
        parse_suggestion(&body)
    }
}

/// Builds the user-turn prompt from the track's text signals, truncating each untrusted field.
fn user_prompt(input: &ClassificationInput) -> String {
    let mut prompt = format!(
        "Title: {}\nArtist: {}",
        truncated(&input.title),
        truncated(&input.artist)
    );
    if let Some(genre) = &input.source_genre {
        prompt.push_str(&format!("\nSource genre tag: {}", truncated(genre)));
    }
    if let Some(description) = &input.description {
        prompt.push_str(&format!("\nDescription: {}", truncated(description)));
    }
    prompt
}

/// Truncates untrusted text to at most `MAX_PROMPT_FIELD_CHARS` characters (char-boundary safe).
fn truncated(field: &str) -> String {
    field.chars().take(MAX_PROMPT_FIELD_CHARS).collect()
}

/// Extracts the first text block, parses its embedded JSON, and maps to a domain suggestion.
fn parse_suggestion(body: &MessagesResponse) -> Result<GenreVibeSuggestion, ClassifyError> {
    let text = body
        .content
        .iter()
        .find_map(|block| block.text.as_deref())
        .ok_or_else(|| ClassifyError::BadResponse {
            source: "no text block in response".into(),
        })?;
    let json = extract_json_object(text).ok_or_else(|| ClassifyError::BadResponse {
        source: "no JSON object in response".into(),
    })?;
    let parsed: SuggestionDto =
        serde_json::from_str(json).map_err(|e| ClassifyError::BadResponse {
            source: Box::new(e),
        })?;
    Ok(dto_to_suggestion(parsed))
}

/// Returns the substring spanning the first `{` to the last `}` (the model may wrap JSON in prose).
fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end > start {
        Some(&text[start..=end])
    } else {
        None
    }
}

/// Maps the parsed DTO to the domain suggestion, clamping confidences into `[0.0, 1.0]`.
fn dto_to_suggestion(dto: SuggestionDto) -> GenreVibeSuggestion {
    let mut candidates = Vec::new();
    for candidate in dto.candidates {
        let clamped = candidate.confidence.clamp(CONFIDENCE_MIN, CONFIDENCE_MAX);
        // `clamp` returns NaN when the model emits a NaN confidence; never panic on model data —
        // fall back to the minimum (which routes the candidate to triage).
        let confidence = Confidence::new(clamped).unwrap_or_else(|_| {
            Confidence::new(CONFIDENCE_MIN).expect("CONFIDENCE_MIN is a valid confidence")
        });
        candidates.push(GenreCandidate {
            genre: candidate.genre,
            confidence,
        });
    }
    GenreVibeSuggestion {
        candidates,
        vibe_tags: dto.vibe_tags,
    }
}

/// Request body for `POST /v1/messages`.
#[derive(Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<Message<'a>>,
}

/// A single conversation message.
#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: String,
}

/// Response body: a list of content blocks.
#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

/// One content block; only `text` blocks carry the answer.
#[derive(Deserialize)]
struct ContentBlock {
    #[serde(default)]
    text: Option<String>,
}

/// The JSON shape the model is instructed to return.
#[derive(Deserialize)]
struct SuggestionDto {
    #[serde(default)]
    candidates: Vec<CandidateDto>,
    #[serde(default)]
    vibe_tags: Vec<String>,
}

/// One candidate genre with a raw (unvalidated) confidence.
#[derive(Deserialize)]
struct CandidateDto {
    genre: String,
    confidence: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_json_wrapped_in_prose() {
        let text = "Here you go: {\"candidates\":[]} done";
        assert_eq!(extract_json_object(text), Some("{\"candidates\":[]}"));
    }

    #[test]
    fn maps_and_clamps_confidence() {
        let dto = SuggestionDto {
            candidates: vec![
                CandidateDto {
                    genre: "Techno".into(),
                    confidence: 1.5,
                },
                CandidateDto {
                    genre: "House".into(),
                    confidence: 0.4,
                },
            ],
            vibe_tags: vec!["dark".into()],
        };
        let suggestion = dto_to_suggestion(dto);
        assert_eq!(suggestion.candidates[0].confidence.value(), 1.0);
        assert_eq!(suggestion.candidates[1].confidence.value(), 0.4);
        assert_eq!(suggestion.vibe_tags, vec!["dark".to_string()]);
    }

    #[test]
    fn nan_confidence_falls_back_to_min_without_panicking() {
        let dto = SuggestionDto {
            candidates: vec![CandidateDto {
                genre: "Techno".into(),
                confidence: f32::NAN,
            }],
            vibe_tags: vec![],
        };
        let suggestion = dto_to_suggestion(dto);
        assert_eq!(suggestion.candidates[0].confidence.value(), CONFIDENCE_MIN);
    }

    #[test]
    fn user_prompt_truncates_long_untrusted_fields() {
        let long = "x".repeat(MAX_PROMPT_FIELD_CHARS + 50);
        let input = ClassificationInput {
            title: long,
            artist: "a".into(),
            source_genre: None,
            description: None,
        };
        let prompt = user_prompt(&input);
        assert!(prompt.contains(&"x".repeat(MAX_PROMPT_FIELD_CHARS)));
        assert!(!prompt.contains(&"x".repeat(MAX_PROMPT_FIELD_CHARS + 1)));
    }
}
