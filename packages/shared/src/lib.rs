//! Code the API and the worker share: where settings come from, the names
//! of the queue and the heartbeat, and the shape of a job.

pub mod job;
pub mod settings;

pub use job::{Job, JobError, MAX_JOB_BYTES};
pub use settings::{
    ApiSettings, HEARTBEAT_SECONDS, JOBS_QUEUE, SettingsError, WORKER_HEARTBEAT, WorkerSettings,
    redacted,
};
