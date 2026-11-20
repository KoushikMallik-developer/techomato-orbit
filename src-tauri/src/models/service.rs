//! The user-defined service: what to run, how to stop it, how to keep it healthy.

use crate::error::{AppError, AppResult, ErrorKind};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Database,
    Cache,
    Backend,
    Frontend,
    Worker,
    Queue,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvVar {
    pub key: String,
    /// `None` for a secret: the value lives in Windows Credential Manager, never in SQLite.
    pub value: Option<String>,
    pub is_secret: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RestartPolicy {
    Never,
    OnFailure { max_retries: u32, backoff_secs: u32 },
    Always { max_retries: u32, backoff_secs: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum HealthCheck {
    Tcp {
        port: u16,
        interval_secs: u32,
        timeout_secs: u32,
    },
    Http {
        url: String,
        min_status: u16,
        max_status: u16,
        interval_secs: u32,
        timeout_secs: u32,
    },
    Command {
        exe: String,
        args: Vec<String>,
        interval_secs: u32,
        timeout_secs: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSpec {
    /// Empty on create; the repository assigns a UUID.
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub category: Category,
    pub project_id: Option<String>,
    pub working_dir: String,
    pub executable: String,
    pub args: Vec<String>,
    pub run_through_shell: bool,
    pub env: Vec<EnvVar>,
    pub port: Option<u16>,
    pub startup_timeout_secs: u32,
    pub shutdown_timeout_secs: u32,
    pub restart: RestartPolicy,
    pub health: Option<HealthCheck>,
    pub keep_running_on_quit: bool,
    pub enabled: bool,
}

fn bad(msg: impl Into<String>) -> AppError {
    AppError::new(ErrorKind::Validation, msg)
}

impl ServiceSpec {
    pub fn validate(&self) -> AppResult<()> {
        if self.name.trim().is_empty() {
            return Err(bad("A service needs a name."));
        }
        if self.executable.trim().is_empty() {
            return Err(bad("A service needs an executable to run."));
        }
        if self.executable.contains('\0') || self.working_dir.contains('\0') {
            return Err(bad(
                "The executable and working directory cannot contain NUL.",
            ));
        }
        if self.args.iter().any(|a| a.contains('\0')) {
            return Err(bad("An argument cannot contain a NUL character."));
        }
        if self.port == Some(0) {
            return Err(bad("The port must be between 1 and 65535."));
        }
        if self.startup_timeout_secs == 0 {
            return Err(bad("The startup timeout must be at least 1 second."));
        }
        if self.shutdown_timeout_secs == 0 {
            return Err(bad("The shutdown timeout must be at least 1 second."));
        }
        let mut seen = HashSet::new();
        for v in &self.env {
            if v.key.is_empty() || v.key.contains('=') || v.key.contains('\0') {
                return Err(bad(format!(
                    "Invalid environment variable name \"{}\": it must be non-empty and cannot contain '='.",
                    v.key
                )));
            }
            if v.value.as_deref().is_some_and(|x| x.contains('\0')) {
                return Err(bad(format!("The value of {} cannot contain NUL.", v.key)));
            }
            if !seen.insert(v.key.to_ascii_uppercase()) {
                return Err(bad(format!(
                    "Environment variable {} is a duplicate.",
                    v.key
                )));
            }
        }
        if let Some(HealthCheck::Http { url, .. }) = &self.health {
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Err(bad(
                    "The health check URL must start with http:// or https://.",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub mod fixtures {
    use super::*;

    /// A valid spec; tests tweak one field at a time.
    pub fn sample() -> ServiceSpec {
        ServiceSpec {
            id: String::new(),
            name: "API".into(),
            description: String::new(),
            category: Category::Backend,
            project_id: None,
            working_dir: "C:/work/api".into(),
            executable: "python".into(),
            args: vec!["-m".into(), "http.server".into()],
            run_through_shell: false,
            env: vec![],
            port: Some(8000),
            startup_timeout_secs: 30,
            shutdown_timeout_secs: 10,
            restart: RestartPolicy::Never,
            health: None,
            keep_running_on_quit: false,
            enabled: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::sample;
    use super::*;

    fn invalid(mutate: impl FnOnce(&mut ServiceSpec)) -> String {
        let mut s = sample();
        mutate(&mut s);
        s.validate().expect_err("should be invalid").message
    }

    #[test]
    fn a_valid_spec_passes() {
        assert!(sample().validate().is_ok());
    }

    #[test]
    fn rejects_empty_name() {
        assert!(invalid(|s| s.name = "  ".into()).contains("name"));
    }

    #[test]
    fn rejects_empty_executable() {
        assert!(invalid(|s| s.executable = String::new()).contains("executable"));
    }

    #[test]
    fn rejects_nul_in_args() {
        assert!(invalid(|s| s.args = vec!["a\0b".into()]).contains("argument"));
    }

    #[test]
    fn rejects_port_zero() {
        assert!(invalid(|s| s.port = Some(0)).contains("port"));
    }

    #[test]
    fn rejects_zero_timeouts() {
        assert!(invalid(|s| s.shutdown_timeout_secs = 0).contains("shutdown"));
        assert!(invalid(|s| s.startup_timeout_secs = 0).contains("startup"));
    }

    #[test]
    fn rejects_bad_env_keys() {
        let bad = |k: &str| {
            invalid(|s| {
                s.env = vec![EnvVar {
                    key: k.into(),
                    value: Some("1".into()),
                    is_secret: false,
                }]
            })
        };
        assert!(bad("").contains("environment"));
        assert!(bad("A=B").contains("environment"));
    }

    #[test]
    fn rejects_duplicate_env_keys() {
        let ev = |k: &str| EnvVar {
            key: k.into(),
            value: Some("1".into()),
            is_secret: false,
        };
        assert!(invalid(|s| s.env = vec![ev("A"), ev("A")]).contains("duplicate"));
    }

    #[test]
    fn rejects_http_health_url_without_scheme() {
        let h = HealthCheck::Http {
            url: "localhost:8000".into(),
            min_status: 200,
            max_status: 399,
            interval_secs: 5,
            timeout_secs: 2,
        };
        assert!(invalid(|s| s.health = Some(h)).contains("http"));
    }

    #[test]
    fn serializes_camel_case_with_tagged_enums() {
        let mut s = sample();
        s.restart = RestartPolicy::OnFailure {
            max_retries: 3,
            backoff_secs: 2,
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["workingDir"], "C:/work/api");
        assert_eq!(v["category"], "backend");
        assert_eq!(v["restart"]["kind"], "onFailure");
        assert_eq!(v["restart"]["maxRetries"], 3);
    }
}
