//! Content reviews apply to exact resource hashes, not arbitrary user phrases.
use serde::Deserialize;
use std::{collections::BTreeMap, sync::OnceLock};
#[derive(Deserialize)]
pub(super) struct Review {
    sha256: String,
    allowed_pairs: BTreeMap<String, String>,
}
impl Review {
    pub(super) fn allows(&self, source: &str, target: &str) -> bool {
        self.allowed_pairs
            .get(source)
            .is_some_and(|value| value == target)
    }
}
fn reviews() -> &'static [Review] {
    static REVIEWS: OnceLock<Vec<Review>> = OnceLock::new();
    REVIEWS.get_or_init(|| {
        serde_json::from_str(include_str!("../../../data/learning/resource-reviews.json"))
            .expect("reviewed resource policy")
    })
}
pub(super) fn find(sha: &str) -> Option<&'static Review> {
    reviews().iter().find(|r| r.sha256 == sha)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reviewed_decisions_are_scoped_to_the_exact_input_artifact() {
        let review = &reviews()[0];
        assert!(review.allows("日本", "일본"));
        assert!(!review.allows("ありがとう", "무관한 표현"));
        assert!(find(&"0".repeat(64)).is_none());
        assert!(find(&review.sha256).is_some());
    }
}
