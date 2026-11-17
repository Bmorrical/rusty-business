//! Starts the worker, or with `healthcheck` asks whether one is alive.

use std::process::ExitCode;
use std::time::Duration;

use redis::{AsyncConnectionConfig, AsyncTypedCommands};
use rusty_shared::{HEARTBEAT_SECONDS, JOBS_QUEUE, WORKER_HEARTBEAT, WorkerSettings};
use rusty_worker::{Outcome, handle};

/// Seconds one wait on the queue lasts before the heartbeat is refreshed.
const WAIT_SECONDS: f64 = 5.0;

/// Longer than one wait: past this, the connection is taken for dead.
const WAIT_LIMIT: Duration = Duration::from_secs(15);

/// The pause before reconnecting after Redis goes away.
const RECONNECT_PAUSE: Duration = Duration::from_secs(2);

#[tokio::main]
async fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        return healthcheck().await;
    }
    tracing_subscriber::fmt().with_ansi(false).init();

    let settings = match WorkerSettings::from_env() {
        Ok(settings) => settings,
        Err(reason) => {
            tracing::error!(%reason, "the worker cannot start");
            return ExitCode::FAILURE;
        }
    };
    tracing::info!(?settings, queue = JOBS_QUEUE, "starting");
    let client = match redis::Client::open(settings.redis_url.as_str()) {
        Ok(client) => client,
        Err(reason) => {
            tracing::error!(%reason, "REDIS_URL is not a usable address");
            return ExitCode::FAILURE;
        }
    };

    tokio::select! {
        _ = asked_to_stop() => {}
        _ = work(&client) => {}
    }
    tracing::info!("stopped");
    ExitCode::SUCCESS
}

/// Waits on the queue for as long as the worker runs, reconnecting whenever
/// Redis goes away.
async fn work(client: &redis::Client) {
    loop {
        if let Err(reason) = wait_on_queue(client).await {
            tracing::warn!(%reason, "lost Redis, reconnecting");
        }
        tokio::time::sleep(RECONNECT_PAUSE).await;
    }
}

async fn wait_on_queue(client: &redis::Client) -> redis::RedisResult<()> {
    // The driver gives up on a reply after half a second by default. A wait
    // on the queue is meant to take longer, so that limit is lifted here and
    // WAIT_LIMIT stands in for it.
    let config = AsyncConnectionConfig::new().set_response_timeout(None);
    let mut connection = client
        .get_multiplexed_async_connection_with_config(&config)
        .await?;
    tracing::info!("waiting on the queue");

    loop {
        connection
            .set_ex(WORKER_HEARTBEAT, "alive", HEARTBEAT_SECONDS)
            .await?;
        let waited =
            tokio::time::timeout(WAIT_LIMIT, connection.blpop(JOBS_QUEUE, WAIT_SECONDS)).await;
        let Ok(arrived) = waited else {
            tracing::warn!("Redis stopped answering");
            return Ok(());
        };
        if let Some([_queue, line]) = arrived? {
            // The payload is never logged: it may hold anything.
            match handle(&line) {
                Outcome::NothingToDo(job) => {
                    tracing::info!(id = %job.id, kind = %job.kind, "job received, nothing to do yet");
                }
                Outcome::Unreadable(reason) => {
                    tracing::warn!(%reason, "dropped a queue entry that is not a job");
                }
            }
        }
    }
}

/// Resolves on Ctrl-C, or on the stop signal Docker sends.
async fn asked_to_stop() {
    let interrupt = async {
        tokio::signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = interrupt => {}
        _ = terminate => {}
    }
}

/// `rusty-worker healthcheck`: exits 0 only while a worker's heartbeat is
/// fresh in Redis.
async fn healthcheck() -> ExitCode {
    match tokio::time::timeout(Duration::from_secs(4), heartbeat_is_fresh()).await {
        Ok(Ok(true)) => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

async fn heartbeat_is_fresh() -> Result<bool, Box<dyn std::error::Error>> {
    let settings = WorkerSettings::from_env()?;
    let client = redis::Client::open(settings.redis_url.as_str())?;
    let mut connection = client.get_multiplexed_async_connection().await?;
    Ok(connection.exists(WORKER_HEARTBEAT).await?)
}
