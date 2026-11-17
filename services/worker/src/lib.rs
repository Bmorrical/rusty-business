//! The worker. It waits on the job queue, reads what arrives, and does
//! nothing with it yet.

use rusty_shared::{Job, JobError};

/// What became of one queue entry.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// A readable job. No kind of job is handled yet, so it is noted and
    /// left at that.
    NothingToDo(Job),
    /// A line that is not a job. It is dropped and the worker carries on.
    Unreadable(JobError),
}

/// Decides what to do with one queue entry. Today the answer is always
/// "nothing"; the first real job kind gets its branch here.
pub fn handle(line: &str) -> Outcome {
    match Job::parse(line) {
        Ok(job) => Outcome::NothingToDo(job),
        Err(reason) => Outcome::Unreadable(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_job_is_read_and_nothing_is_done_with_it() {
        let outcome = handle(r#"{"id":"0001","kind":"hello","payload":{"n":1}}"#);
        let Outcome::NothingToDo(job) = outcome else {
            panic!("a readable job should be noted, got {outcome:?}");
        };
        assert_eq!(job.id, "0001");
        assert_eq!(job.kind, "hello");
    }

    #[test]
    fn a_line_that_is_not_a_job_is_dropped_without_stopping_the_worker() {
        for line in [
            "",
            "not json",
            r#"{"id":"1"}"#,
            r#"{"id":"1","kind":"a b"}"#,
        ] {
            assert!(
                matches!(handle(line), Outcome::Unreadable(_)),
                "{line:?} should be dropped"
            );
        }
    }
}
