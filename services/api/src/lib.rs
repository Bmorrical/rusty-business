//! The API. Today it answers two questions: who are you, and are you well.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

pub mod probes;

pub const SERVICE: &str = "rusty-business-api";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// How long one dependency gets to answer before it counts as down.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// The starter migration and every one after it, built into the program.
pub static MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// The things the API cannot work without. Each answers true when it can be
/// reached. The live ones are in [`probes`]; the tests supply their own.
pub trait Probes: Send + Sync + 'static {
    fn postgres(&self) -> impl Future<Output = bool> + Send;
    fn redis(&self) -> impl Future<Output = bool> + Send;
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct About {
    pub service: &'static str,
    pub version: &'static str,
}

/// What `/health` says. A check is "ok" or "down" and nothing more: the
/// reason goes to the log, never to the caller.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Health {
    pub status: &'static str,
    pub checks: Checks,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Checks {
    pub postgres: &'static str,
    pub redis: &'static str,
}

impl Health {
    pub fn from_checks(postgres: bool, redis: bool) -> Self {
        Self {
            status: word(postgres && redis),
            checks: Checks {
                postgres: word(postgres),
                redis: word(redis),
            },
        }
    }

    pub fn is_ok(&self) -> bool {
        self.status == "ok"
    }
}

fn word(up: bool) -> &'static str {
    if up { "ok" } else { "down" }
}

/// Every route the API has.
pub fn router<P: Probes>(probes: P) -> Router {
    Router::new()
        .route("/", get(about))
        .route("/health", get(health::<P>))
        .with_state(Arc::new(probes))
}

async fn about() -> Json<About> {
    Json(About {
        service: SERVICE,
        version: VERSION,
    })
}

async fn health<P: Probes>(State(probes): State<Arc<P>>) -> (StatusCode, Json<Health>) {
    let (postgres, redis) = tokio::join!(answered(probes.postgres()), answered(probes.redis()));
    let health = Health::from_checks(postgres, redis);
    let code = if health.is_ok() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (code, Json(health))
}

/// A probe that takes too long counts as down, so `/health` always answers.
async fn answered(probe: impl Future<Output = bool>) -> bool {
    tokio::time::timeout(PROBE_TIMEOUT, probe)
        .await
        .unwrap_or(false)
}

/// True when the first line of an http reply says 200. The container's own
/// health check uses it.
pub fn is_ok_reply(head: &str) -> bool {
    let mut parts = head.split_whitespace();
    matches!(parts.next(), Some(version) if version.starts_with("HTTP/"))
        && parts.next() == Some("200")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    /// Stand-in dependencies: each one is up, down, or never answers.
    #[derive(Clone, Copy)]
    enum Fake {
        Up,
        Down,
        Silent,
    }

    impl Fake {
        async fn answer(self) -> bool {
            match self {
                Fake::Up => true,
                Fake::Down => false,
                Fake::Silent => std::future::pending().await,
            }
        }
    }

    struct FakeProbes {
        postgres: Fake,
        redis: Fake,
    }

    impl Probes for FakeProbes {
        async fn postgres(&self) -> bool {
            self.postgres.answer().await
        }
        async fn redis(&self) -> bool {
            self.redis.answer().await
        }
    }

    async fn get_json(postgres: Fake, redis: Fake, path: &str) -> (StatusCode, serde_json::Value) {
        let reply = router(FakeProbes { postgres, redis })
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = reply.status();
        let bytes = reply.into_body().collect().await.unwrap().to_bytes();
        let json = if bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, json)
    }

    #[tokio::test]
    async fn the_front_page_says_who_is_answering() {
        let (status, body) = get_json(Fake::Up, Fake::Up, "/").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            serde_json::json!({"service": "rusty-business-api", "version": VERSION})
        );
    }

    #[tokio::test]
    async fn health_is_ok_when_postgres_and_redis_answer() {
        let (status, body) = get_json(Fake::Up, Fake::Up, "/health").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            serde_json::json!({"status": "ok", "checks": {"postgres": "ok", "redis": "ok"}})
        );
    }

    #[tokio::test]
    async fn health_says_which_one_is_down() {
        let (status, body) = get_json(Fake::Down, Fake::Up, "/health").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            body,
            serde_json::json!({"status": "down", "checks": {"postgres": "down", "redis": "ok"}})
        );

        let (status, body) = get_json(Fake::Up, Fake::Down, "/health").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            body,
            serde_json::json!({"status": "down", "checks": {"postgres": "ok", "redis": "down"}})
        );
    }

    // Time is paused, so the wait for the silent one costs the test nothing.
    #[tokio::test(start_paused = true)]
    async fn a_dependency_that_never_answers_counts_as_down() {
        let (status, body) = get_json(Fake::Up, Fake::Silent, "/health").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["checks"]["postgres"], "ok");
        assert_eq!(body["checks"]["redis"], "down");
    }

    #[tokio::test]
    async fn a_path_that_does_not_exist_is_not_found() {
        let (status, _) = get_json(Fake::Up, Fake::Up, "/nothing-here").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[test]
    fn only_a_200_reply_passes_the_container_health_check() {
        assert!(is_ok_reply(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json"
        ));
        assert!(is_ok_reply("HTTP/1.0 200 OK"));
        assert!(!is_ok_reply("HTTP/1.1 503 Service Unavailable"));
        assert!(!is_ok_reply("HTTP/1.1 404 Not Found"));
        assert!(!is_ok_reply("200"));
        assert!(!is_ok_reply(""));
    }

    #[test]
    fn the_starter_migration_is_built_in() {
        let versions: Vec<i64> = MIGRATIONS.iter().map(|m| m.version).collect();
        assert_eq!(
            versions,
            vec![1],
            "the migrations folder holds one migration"
        );
    }
}
