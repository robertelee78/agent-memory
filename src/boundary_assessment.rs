//! Validate agent-authored no-learning assessments, not semantic truth.
use crate::codec::json::{self, Value};
use crate::error::{ContractError, Result};

fn keys(value: &Value, expected: &[&str]) -> bool {
    value.as_object().is_some_and(|pairs| {
        pairs.len() == expected.len() && expected.iter().all(|key| value.get(key).is_some())
    })
}

// Python str.strip includes the four information separators as whitespace.
fn nonempty(value: &Value, maximum: usize) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() <= maximum
            && !s
                .trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
                .is_empty()
    })
}

pub fn validate<'a>(value: &'a Value, source: &str) -> Result<&'a Value> {
    if !keys(value, &["source_id", "source_review", "candidates"])
        || value.get("source_id").and_then(Value::as_str) != Some(source)
        || !value
            .get("source_review")
            .is_some_and(|v| nonempty(v, 2048))
        || !value
            .get("candidates")
            .and_then(Value::as_array)
            .is_some_and(|v| v.len() <= 32)
    {
        return Err(ContractError::new(
            "Use a bounded source-bound no-learning assessment",
        ));
    }
    if json::encode(value, json::UTF8)?.len() > 2048 {
        return Err(ContractError::new(
            "No-learning assessment exceeds its bound",
        ));
    }
    for candidate in value.get("candidates").and_then(Value::as_array).unwrap() {
        let disposition = candidate.get("disposition").and_then(Value::as_str);
        let expected: &[&str] = if disposition == Some("already-recorded") {
            &["claim", "disposition", "reason", "memory_evidence"]
        } else {
            &["claim", "disposition", "reason"]
        };
        if !matches!(disposition, Some("not-durable" | "already-recorded"))
            || !keys(candidate, expected)
            || expected
                .iter()
                .filter(|&&key| key != "disposition")
                .any(|key| !candidate.get(key).is_some_and(|v| nonempty(v, 1024)))
        {
            return Err(ContractError::new(
                "Eligible, ambiguous or unresolved learning remains pending",
            ));
        }
    }
    Ok(value)
}
