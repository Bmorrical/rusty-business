//! The live checks behind `/health`: one question to Postgres, one to Redis.

use sqlx::PgPool;

use crate::Probes;

pub struct LiveProbes {
    pool: PgPool,
    redis: redis::Client,
}

impl LiveProbes {
    pub fn new(pool: PgPool, redis: redis::Client) -> Self {
        Self { pool, redis }
    }
}

impl Probes for LiveProbes {
    async fn postgres(&self) -> bool {
        match sqlx::query("SELECT 1").execute(&self.pool).await {
            Ok(_) => true,
            Err(reason) => {
                tracing::warn!(%reason, "Postgres did not answer");
                false
            }
        }
    }

    async fn redis(&self) -> bool {
        match ping(&self.redis).await {
            Ok(()) => true,
            Err(reason) => {
                tracing::warn!(%reason, "Redis did not answer");
                false
            }
        }
    }
}

/// Opens a fresh connection each time, so the answer is about Redis now and
/// not about a connection made earlier.
async fn ping(client: &redis::Client) -> redis::RedisResult<()> {
    let mut connection = client.get_multiplexed_async_connection().await?;
    redis::cmd("PING")
        .query_async::<String>(&mut connection)
        .await?;
    Ok(())
}

/// These talk to the real Postgres and Redis, so they are skipped unless
/// asked for. With the stack up:
///
/// ```bash
/// make test-live
/// ```
#[cfg(test)]
mod live {
    use super::*;
    use crate::{MIGRATIONS, router};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use rusty_shared::ApiSettings;
    use tower::ServiceExt;

    async fn probes() -> (LiveProbes, PgPool) {
        let settings = ApiSettings::from_env().expect("DATABASE_URL and REDIS_URL are set");
        let pool = PgPool::connect(&settings.database_url).await.unwrap();
        let redis = redis::Client::open(settings.redis_url.as_str()).unwrap();
        (LiveProbes::new(pool.clone(), redis), pool)
    }

    #[tokio::test]
    #[ignore = "needs the stack: make test-live"]
    async fn live_health_is_ok() {
        let (probes, _) = probes().await;
        let reply = router(probes)
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(reply.status(), StatusCode::OK);
    }

    #[tokio::test]
    #[ignore = "needs the stack: make test-live"]
    async fn live_starter_migration_has_run() {
        let (_, pool) = probes().await;
        // Running it again changes nothing: a migration runs once.
        MIGRATIONS.run(&pool).await.unwrap();
        let notes: i64 = sqlx::query_scalar("SELECT count(*) FROM bootstrap_notes")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(notes, 1);
    }
}
