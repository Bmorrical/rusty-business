//! Settings come from the environment and nowhere else. No address and no
//! password has a default here: a missing value stops the service at start
//! and names what is missing.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr};

/// The Redis list the worker waits on. One JSON job per entry.
pub const JOBS_QUEUE: &str = "rusty:jobs";

/// A key the worker keeps fresh while it is alive.
pub const WORKER_HEARTBEAT: &str = "rusty:worker:heartbeat";

/// Seconds the heartbeat outlives a worker that has stopped.
pub const HEARTBEAT_SECONDS: u64 = 30;

/// Where the API listens unless told otherwise: this computer only.
const DEFAULT_API_BIND: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const DEFAULT_API_PORT: u16 = 8000;

#[derive(Debug, PartialEq, Eq)]
pub enum SettingsError {
    /// The named value is not set, or is empty.
    Missing(&'static str),
    /// The named value is set to something that cannot be used.
    Invalid {
        name: &'static str,
        expected: &'static str,
    },
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(name) => write!(f, "{name} is not set"),
            Self::Invalid { name, expected } => write!(f, "{name} should be {expected}"),
        }
    }
}

impl std::error::Error for SettingsError {}

/// What the API needs to start.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiSettings {
    pub database_url: String,
    pub redis_url: String,
    pub bind: IpAddr,
    pub port: u16,
}

/// What the worker needs to start.
#[derive(Clone, PartialEq, Eq)]
pub struct WorkerSettings {
    pub redis_url: String,
}

impl ApiSettings {
    pub fn from_env() -> Result<Self, SettingsError> {
        Self::from_lookup(from_environment)
    }

    /// Reads the settings through `lookup`, so a test can supply its own.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, SettingsError> {
        Ok(Self {
            database_url: database_url(&lookup)?,
            redis_url: redis_url(&lookup)?,
            bind: match optional(&lookup, "API_BIND") {
                None => DEFAULT_API_BIND,
                Some(text) => text.parse().map_err(|_| SettingsError::Invalid {
                    name: "API_BIND",
                    expected: "an IP address, such as 127.0.0.1",
                })?,
            },
            port: api_port(&lookup)?,
        })
    }
}

impl WorkerSettings {
    pub fn from_env() -> Result<Self, SettingsError> {
        Self::from_lookup(from_environment)
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, SettingsError> {
        Ok(Self {
            redis_url: redis_url(&lookup)?,
        })
    }
}

/// The port the API listens on. Its own health check reads this too.
pub fn api_port(lookup: &impl Fn(&str) -> Option<String>) -> Result<u16, SettingsError> {
    match optional(lookup, "API_PORT") {
        None => Ok(DEFAULT_API_PORT),
        Some(text) => match text.parse::<u16>() {
            Ok(port) if port > 0 => Ok(port),
            _ => Err(SettingsError::Invalid {
                name: "API_PORT",
                expected: "a port number from 1 to 65535",
            }),
        },
    }
}

/// The lookup the services use outside of tests.
pub fn from_environment(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// An address with its password hidden, safe to put in a log line.
pub fn redacted(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return "***".to_string();
    };
    let Some((credentials, host)) = rest.rsplit_once('@') else {
        return url.to_string();
    };
    match credentials.split_once(':') {
        Some((user, _password)) => format!("{scheme}://{user}:***@{host}"),
        None => format!("{scheme}://{credentials}@{host}"),
    }
}

// Debug is written by hand so that a settings value in a log line never
// carries a password.
impl fmt::Debug for ApiSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiSettings")
            .field("database_url", &redacted(&self.database_url))
            .field("redis_url", &redacted(&self.redis_url))
            .field("bind", &self.bind)
            .field("port", &self.port)
            .finish()
    }
}

impl fmt::Debug for WorkerSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WorkerSettings")
            .field("redis_url", &redacted(&self.redis_url))
            .finish()
    }
}

fn optional(lookup: &impl Fn(&str) -> Option<String>, name: &str) -> Option<String> {
    lookup(name)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn database_url(lookup: &impl Fn(&str) -> Option<String>) -> Result<String, SettingsError> {
    address(
        lookup,
        "DATABASE_URL",
        &["postgres://", "postgresql://"],
        "a Postgres address that starts with postgres://",
    )
}

fn redis_url(lookup: &impl Fn(&str) -> Option<String>) -> Result<String, SettingsError> {
    address(
        lookup,
        "REDIS_URL",
        &["redis://", "rediss://"],
        "a Redis address that starts with redis://",
    )
}

fn address(
    lookup: &impl Fn(&str) -> Option<String>,
    name: &'static str,
    schemes: &[&str],
    expected: &'static str,
) -> Result<String, SettingsError> {
    let value = optional(lookup, name).ok_or(SettingsError::Missing(name))?;
    if schemes.iter().any(|scheme| value.starts_with(scheme)) {
        Ok(value)
    } else {
        Err(SettingsError::Invalid { name, expected })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATABASE: &str = "postgres://rusty:s3cret@postgres:5432/rusty";
    const REDIS: &str = "redis://:hunter2@redis:6379/0";

    /// A lookup over a fixed list, standing in for the environment.
    fn lookup(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        }
    }

    #[test]
    fn the_api_reads_its_settings() {
        let settings = ApiSettings::from_lookup(lookup(&[
            ("DATABASE_URL", DATABASE),
            ("REDIS_URL", REDIS),
            ("API_BIND", "0.0.0.0"),
            ("API_PORT", "9100"),
        ]))
        .unwrap();
        assert_eq!(settings.database_url, DATABASE);
        assert_eq!(settings.redis_url, REDIS);
        assert_eq!(settings.bind.to_string(), "0.0.0.0");
        assert_eq!(settings.port, 9100);
    }

    #[test]
    fn the_api_listens_on_this_computer_only_unless_told_otherwise() {
        let settings =
            ApiSettings::from_lookup(lookup(&[("DATABASE_URL", DATABASE), ("REDIS_URL", REDIS)]))
                .unwrap();
        assert!(settings.bind.is_loopback());
        assert_eq!(settings.port, 8000);
    }

    #[test]
    fn a_missing_value_is_named() {
        let missing = ApiSettings::from_lookup(lookup(&[("REDIS_URL", REDIS)])).unwrap_err();
        assert_eq!(missing, SettingsError::Missing("DATABASE_URL"));
        assert_eq!(missing.to_string(), "DATABASE_URL is not set");

        let empty = WorkerSettings::from_lookup(lookup(&[("REDIS_URL", "   ")])).unwrap_err();
        assert_eq!(empty, SettingsError::Missing("REDIS_URL"));
    }

    #[test]
    fn an_address_of_the_wrong_kind_is_refused() {
        let wrong = ApiSettings::from_lookup(lookup(&[
            ("DATABASE_URL", "mysql://rusty:s3cret@db/rusty"),
            ("REDIS_URL", REDIS),
        ]))
        .unwrap_err();
        assert!(matches!(
            wrong,
            SettingsError::Invalid {
                name: "DATABASE_URL",
                ..
            }
        ));
        // The refusal says what is expected and never repeats the value.
        assert!(!wrong.to_string().contains("s3cret"));
    }

    #[test]
    fn a_bad_port_or_bind_address_is_refused() {
        for port in ["0", "70000", "http", "-1"] {
            let pairs: &'static [(&str, &str)] = Box::leak(Box::new([
                ("DATABASE_URL", DATABASE),
                ("REDIS_URL", REDIS),
                ("API_PORT", port),
            ]));
            assert!(
                matches!(
                    ApiSettings::from_lookup(lookup(pairs)),
                    Err(SettingsError::Invalid {
                        name: "API_PORT",
                        ..
                    })
                ),
                "port {port} should be refused"
            );
        }
        let bind = ApiSettings::from_lookup(lookup(&[
            ("DATABASE_URL", DATABASE),
            ("REDIS_URL", REDIS),
            ("API_BIND", "everywhere"),
        ]));
        assert!(matches!(
            bind,
            Err(SettingsError::Invalid {
                name: "API_BIND",
                ..
            })
        ));
    }

    #[test]
    fn the_worker_needs_only_redis() {
        let settings = WorkerSettings::from_lookup(lookup(&[("REDIS_URL", REDIS)])).unwrap();
        assert_eq!(settings.redis_url, REDIS);
    }

    #[test]
    fn a_password_never_reaches_a_log_line() {
        let api =
            ApiSettings::from_lookup(lookup(&[("DATABASE_URL", DATABASE), ("REDIS_URL", REDIS)]))
                .unwrap();
        let worker = WorkerSettings::from_lookup(lookup(&[("REDIS_URL", REDIS)])).unwrap();
        let printed = format!("{api:?} {worker:?}");
        assert!(!printed.contains("s3cret"), "{printed}");
        assert!(!printed.contains("hunter2"), "{printed}");
        assert!(printed.contains("postgres://rusty:***@postgres:5432/rusty"));
        assert!(printed.contains("redis://:***@redis:6379/0"));
    }

    #[test]
    fn redacting_keeps_what_is_not_secret() {
        assert_eq!(redacted("redis://redis:6379/0"), "redis://redis:6379/0");
        assert_eq!(
            redacted("postgres://rusty@db/rusty"),
            "postgres://rusty@db/rusty"
        );
        // A password with an at sign in it is still hidden whole.
        assert_eq!(redacted("postgres://u:p@ss@db/x"), "postgres://u:***@db/x");
        // Something that is not an address at all is not echoed.
        assert_eq!(redacted("s3cret"), "***");
    }
}
