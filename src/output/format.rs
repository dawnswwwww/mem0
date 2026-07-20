use serde_json::{json, Value};

use crate::core::error::MemError;
use crate::store::memories::MemoryItem;

pub fn memory_human_line(m: &MemoryItem) -> String {
    let short = &m.id.to_string()[..8];
    format!("[{}] {:>8}  {}", short, m.lifecycle, m.content)
}

pub fn memory_json(m: &MemoryItem) -> Value {
    json!({
        "id": m.id.to_string(),
        "lifecycle": m.lifecycle.to_string(),
        "content": m.content,
        "source": m.source,
        "session_id": m.session_id.map(|u| u.to_string()),
        "tags": m.tags,
        "created_at": m.created_at,
        "updated_at": m.updated_at,
        "accessed_at": m.accessed_at,
    })
}

/// One-line human rendering with a trailing distance, for `vsearch`.
pub fn vsearch_line(m: &MemoryItem, distance: f64) -> String {
    format!("{} (distance={})", memory_human_line(m), distance)
}

/// Structured rendering with distance, for `vsearch --json`.
pub fn memory_json_with_distance(m: &MemoryItem, distance: f64) -> Value {
    let mut v = memory_json(m);
    v["distance"] = json!(distance);
    v
}

/// JSON list wrapper for vsearch hits, each carrying its distance.
pub fn vsearch_json(hits: &[(&MemoryItem, f64)]) -> Value {
    let arr: Vec<Value> = hits
        .iter()
        .map(|(m, d)| memory_json_with_distance(m, *d))
        .collect();
    json!({ "items": arr, "count": arr.len() })
}

pub fn list_json(items: &[MemoryItem]) -> Value {
    let arr: Vec<Value> = items.iter().map(memory_json).collect();
    json!({ "items": arr, "count": arr.len() })
}

pub fn error_json(e: &MemError) -> Value {
    let code = match e {
        MemError::NotFound(_)              => "NotFound",
        MemError::InvalidId(_)             => "InvalidId",
        MemError::InvalidTransition { .. } => "InvalidTransition",
        MemError::InvalidArgument(_)       => "InvalidArgument",
        MemError::EmbeddingDimMismatch { .. } => "EmbeddingDimMismatch",
        MemError::EmbeddingParseError(_)      => "EmbeddingParseError",
        MemError::VectorNotInitialized        => "VectorNotInitialized",
        MemError::EmbedderInitError(_)        => "EmbedderInitError",
        MemError::EmbedderInferenceError(_)   => "EmbedderInferenceError",
        MemError::EmbedFeatureNotEnabled      => "EmbedFeatureNotEnabled",
        MemError::Storage(_)               => "Storage",
        MemError::Io(_)                    => "Io",
        MemError::Json(_)                  => "Json",
    };
    json!({ "error": { "code": code, "message": e.to_string() } })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Lifecycle;
    use crate::store::memories::MemoryItem;

    fn sample(id: &str, lc: Lifecycle, content: &str) -> MemoryItem {
        let id = uuid::Uuid::parse_str(id).unwrap();
        MemoryItem {
            id, lifecycle: lc, content: content.to_string(),
            source: None, session_id: None, tags: vec![],
            created_at: 1_700_000_000_000_000_000, updated_at: 1_700_000_000_000_000_000,
            accessed_at: None,
        }
    }

    #[test]
    fn human_one_per_line_with_short_id_and_layer() {
        let m = sample("11111111-2222-3333-4444-555555555555", Lifecycle::Semantic, "user likes whiskey");
        let line = memory_human_line(&m);
        assert!(line.contains("11111111"), "missing id prefix: {line}");
        assert!(line.contains("semantic"),   "missing layer: {line}");
        assert!(line.contains("user likes whiskey"));
    }

    #[test]
    fn json_item_has_stable_fields() {
        let m = sample("11111111-2222-3333-4444-555555555555", Lifecycle::Semantic, "x");
        let v = memory_json(&m);
        assert_eq!(v["id"], "11111111-2222-3333-4444-555555555555");
        assert_eq!(v["lifecycle"], "semantic");
        assert_eq!(v["content"], "x");
    }

    #[test]
    fn json_list_wraps_items_and_count() {
        let items = vec![
            sample("11111111-2222-3333-4444-555555555555", Lifecycle::Semantic, "a"),
            sample("22222222-2222-3333-4444-555555555555", Lifecycle::Working,  "b"),
        ];
        let v = list_json(&items);
        assert_eq!(v["count"], 2);
        assert_eq!(v["items"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn json_error_shape() {
        let e = MemError::NotFound("abc12345".into());
        let v = error_json(&e);
        assert_eq!(v["error"]["code"], "NotFound");
        assert!(v["error"]["message"].as_str().unwrap().contains("abc12345"));
    }

    #[test]
    fn vsearch_line_includes_distance() {
        let m = sample("11111111-2222-3333-4444-555555555555", Lifecycle::Semantic, "x");
        let line = vsearch_line(&m, 0.123);
        assert!(line.contains("x"));
        assert!(line.contains("0.123"), "missing distance: {line}");
    }

    #[test]
    fn json_with_distance_has_distance_field() {
        let m = sample("11111111-2222-3333-4444-555555555555", Lifecycle::Semantic, "x");
        let v = memory_json_with_distance(&m, 0.5);
        assert_eq!(v["distance"], 0.5);
    }
}
