//! T026 — `GenreVibeClassifierPort` contract. The in-memory fake always runs; the real Anthropic
//! adapter runs only under `--features real-adapters` (needs `ANTHROPIC_API_KEY` + network).

mod support;

use application::ports::genre_vibe_classifier::{GenreCandidate, GenreVibeSuggestion};
use application::testkit::stub_genre_vibe_classifier::StubGenreVibeClassifier;
use domain::confidence::Confidence;

#[tokio::test]
async fn in_memory_classifier_honors_contract() {
    let suggestion = GenreVibeSuggestion {
        candidates: vec![GenreCandidate {
            genre: "House".into(),
            confidence: Confidence::new(0.8).unwrap(),
        }],
        vibe_tags: vec!["warm".into()],
    };
    let classifier = StubGenreVibeClassifier::always(suggestion);
    support::genre_vibe_classifier_suite(&classifier).await;
}

#[cfg(feature = "real-adapters")]
#[tokio::test]
async fn real_classifier_honors_contract() {
    use adapters::anthropic_genre_vibe_classifier::AnthropicGenreVibeClassifier;
    let api_key = std::env::var("ANTHROPIC_API_KEY").expect("set ANTHROPIC_API_KEY");
    let classifier = AnthropicGenreVibeClassifier::new(api_key);
    support::genre_vibe_classifier_suite(&classifier).await;
}
