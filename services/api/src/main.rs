//! Starts the API, or with `healthcheck` asks a running one how it is.

use std::net::{Ipv4Addr, SocketAddr};
use std::process::ExitCode;
use std::time::Duration;

use rusty_api::probes::LiveProbes;
use rusty_api::{MIGRATIONS, is_ok_reply, router};
use rusty_shared::ApiSettings;
use rusty_shared::settings::{api_port, from_environment};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Postgres may still be starting when the API does.
const CONNECT_TRIES: u32 = 15;
const CONNECT_PAUSE: Duration = Duration::from_secs(2);

#[tokio::main]
async fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        return healthcheck().await;
    }
    tracing_subscriber::fmt().with_ansi(false).init();
    match serve().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(reason) => {
            tracing::error!(%reason, "the API stopped");
            ExitCode::FAILURE
        }
    }
}

async fn serve() -> Result<(), Box<dyn std::error::Error>> {
    let settings = ApiSettings::from_env()?;
    tracing::info!(?settings, "starting");

    let pool = connect(&settings.database_url).await?;
    MIGRATIONS.run(&pool).await?;
    tracing::info!("migrations are up to date");

    let redis = redis::Client::open(settings.redis_url.as_str())?;
    let listener = TcpListener::bind(SocketAddr::new(settings.bind, settings.port)).await?;
    tracing::info!(address = %listener.local_addr()?, "listening");

    axum::serve(listener, router(LiveProbes::new(pool, redis)))
        .with_graceful_shutdown(asked_to_stop())
        .await?;
    tracing::info!("stopped");
    Ok(())
}

async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    let mut tries = 0;
    loop {
        tries += 1;
        let attempt = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(5))
            .connect(database_url)
            .await;
        match attempt {
            Ok(pool) => return Ok(pool),
            Err(reason) if tries < CONNECT_TRIES => {
                tracing::warn!(%reason, tries, "Postgres is not ready, trying again");
                tokio::time::sleep(CONNECT_PAUSE).await;
            }
            Err(reason) => return Err(reason),
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

/// `rusty-api healthcheck`: asks the API on this machine for `/health` and
/// exits 0 only on a 200. The container image has no curl, so the program
/// checks itself.
async fn healthcheck() -> ExitCode {
    let Ok(port) = api_port(&from_environment) else {
        return ExitCode::FAILURE;
    };
    match tokio::time::timeout(Duration::from_secs(4), ask_health(port)).await {
        Ok(Ok(true)) => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

async fn ask_health(port: u16) -> std::io::Result<bool> {
    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await?;
    stream
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await?;
    let mut head = [0u8; 64];
    let mut filled = 0;
    while filled < head.len() {
        let read = stream.read(&mut head[filled..]).await?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    Ok(is_ok_reply(&String::from_utf8_lossy(&head[..filled])))
}
