//! A job as it sits on the queue: one JSON object per list entry.
//!
//! ```json
//! {"id": "0001", "kind": "hello", "payload": {"any": "json"}}
//! ```

use std::fmt;

use serde::{Deserialize, Serialize};

/// The longest line the worker will read as a job.
pub const MAX_JOB_BYTES: usize = 64 * 1024;

/// The longest `id` or `kind`.
const MAX_NAME_CHARS: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    /// Whatever the sender uses to tell its jobs apart.
    pub id: String,
    /// What the job asks for. No kind is handled yet.
    pub kind: String,
    /// Anything the job carries. The worker never logs it.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub payload: serde_json::Value,
}

#[derive(Debug, PartialEq, Eq)]
pub enum JobError {
    /// The line is longer than [`MAX_JOB_BYTES`].
    TooLong,
    /// The line is not a JSON object with an `id` and a `kind`.
    NotAJob(String),
    /// The named field is empty, too long, or holds a character outside
    /// letters, digits, and `. _ : -`.
    BadName(&'static str),
}

impl fmt::Display for JobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong => write!(f, "the line is longer than {MAX_JOB_BYTES} bytes"),
            Self::NotAJob(reason) => write!(f, "the line is not a job: {reason}"),
            Self::BadName(field) => write!(
                f,
                "{field} must be 1 to {MAX_NAME_CHARS} letters, digits, dots, underscores, colons, or hyphens"
            ),
        }
    }
}

impl std::error::Error for JobError {}

impl Job {
    /// Reads one queue entry.
    pub fn parse(line: &str) -> Result<Self, JobError> {
        if line.len() > MAX_JOB_BYTES {
            return Err(JobError::TooLong);
        }
        let job: Job =
            serde_json::from_str(line).map_err(|error| JobError::NotAJob(error.to_string()))?;
        if !is_name(&job.id) {
            return Err(JobError::BadName("id"));
        }
        if !is_name(&job.kind) {
            return Err(JobError::BadName("kind"));
        }
        Ok(job)
    }

    /// The job as one queue entry.
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).expect("a job is always valid JSON")
    }
}

fn is_name(text: &str) -> bool {
    !text.is_empty()
        && text.chars().count() <= MAX_NAME_CHARS
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_job_comes_back_as_it_went_in() {
        let job = Job {
            id: "0001".into(),
            kind: "hello".into(),
            payload: serde_json::json!({"to": "the queue"}),
        };
        assert_eq!(Job::parse(&job.to_line()), Ok(job));
    }

    #[test]
    fn a_job_needs_no_payload() {
        let job = Job::parse(r#"{"id":"a-1","kind":"mail.send"}"#).unwrap();
        assert_eq!(job.payload, serde_json::Value::Null);
        assert_eq!(job.to_line(), r#"{"id":"a-1","kind":"mail.send"}"#);
    }

    #[test]
    fn a_line_that_is_not_a_job_is_refused() {
        for line in [
            "",
            "hello",
            "[]",
            r#"{"id":"1"}"#,
            r#"{"kind":"hello"}"#,
            r#"{"id":1,"kind":"hello"}"#,
            r#"{"id":"1","kind":"hello","extra":true}"#,
        ] {
            assert!(
                matches!(Job::parse(line), Err(JobError::NotAJob(_))),
                "{line:?} should not read as a job"
            );
        }
    }

    #[test]
    fn an_id_or_kind_that_is_not_a_plain_name_is_refused() {
        assert_eq!(
            Job::parse(r#"{"id":"","kind":"hello"}"#),
            Err(JobError::BadName("id"))
        );
        assert_eq!(
            Job::parse(r#"{"id":"1","kind":"rm -rf /"}"#),
            Err(JobError::BadName("kind"))
        );
        let long = "k".repeat(MAX_NAME_CHARS + 1);
        assert_eq!(
            Job::parse(&format!(r#"{{"id":"1","kind":"{long}"}}"#)),
            Err(JobError::BadName("kind"))
        );
    }

    #[test]
    fn a_line_past_the_limit_is_refused_before_it_is_read() {
        let payload = "x".repeat(MAX_JOB_BYTES);
        let line = format!(r#"{{"id":"1","kind":"hello","payload":"{payload}"}}"#);
        assert_eq!(Job::parse(&line), Err(JobError::TooLong));
    }
}
