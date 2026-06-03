//! App Health Evaluator — W2.6
//!
//! Collects application health reports, applies threshold policies, and
//! triggers lifecycle actions (restart / rollback / alert).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use agent_core::chain::AuditWriter;
use agent_core::security::{Action, AuditEvent, Principal, Resource, Role};
use tokio::sync::mpsc;
use tracing::warn;

fn digest_params(value: serde_json::Value) -> String {
    use ring::digest::{SHA256, digest};
    let bytes = serde_json::to_vec(&value).unwrap_or_default();
    base16::encode_lower(digest(&SHA256, &bytes).as_ref())
}

// ── health status ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

impl From<i32> for HealthStatus {
    fn from(v: i32) -> Self {
        match v {
            1 => Self::Healthy,
            2 => Self::Degraded,
            3 => Self::Unhealthy,
            _ => Self::Unknown,
        }
    }
}

// ── policy ──────────────────────────────────────────────────────────────────

/// Per-application failure policy.
#[derive(Debug, Clone)]
pub struct HealthPolicy {
    /// Number of consecutive unhealthy reports before acting.
    pub unhealthy_threshold: u32,
    /// Action to take when threshold is crossed.
    pub action: PolicyAction,
    /// Minimum interval between automatic restarts.
    pub min_restart_interval: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyAction {
    Restart,
    Alert,
    RestartThenAlert,
}

impl Default for HealthPolicy {
    fn default() -> Self {
        Self {
            unhealthy_threshold: 3,
            action: PolicyAction::RestartThenAlert,
            min_restart_interval: Duration::from_secs(30),
        }
    }
}

// ── per-app tracking ─────────────────────────────────────────────────────────

struct AppHealth {
    consecutive_failures: u32,
    last_status: HealthStatus,
    last_restart: Option<Instant>,
    policy: HealthPolicy,
}

// ── evaluator ────────────────────────────────────────────────────────────────

/// Event emitted when a policy fires.
#[derive(Debug, Clone)]
pub struct HealthAction {
    pub app_id: String,
    pub action: PolicyAction,
    pub consecutive_failures: u32,
}

pub struct HealthEvaluator {
    apps: Mutex<HashMap<String, AppHealth>>,
    action_tx: mpsc::Sender<HealthAction>,
    audit: Option<HealthAudit>,
}

#[derive(Clone)]
struct HealthAudit {
    device_id: String,
    writer: AuditWriter,
}

impl HealthEvaluator {
    pub fn new(action_tx: mpsc::Sender<HealthAction>) -> Arc<Self> {
        Arc::new(Self {
            apps: Mutex::new(HashMap::new()),
            action_tx,
            audit: None,
        })
    }

    pub fn with_audit(
        self: Arc<Self>,
        device_id: impl Into<String>,
        writer: AuditWriter,
    ) -> Arc<Self> {
        Arc::new(Self {
            apps: Mutex::new(
                self.apps
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|(app_id, health)| {
                        (
                            app_id.clone(),
                            AppHealth {
                                consecutive_failures: health.consecutive_failures,
                                last_status: health.last_status.clone(),
                                last_restart: health.last_restart,
                                policy: health.policy.clone(),
                            },
                        )
                    })
                    .collect(),
            ),
            action_tx: self.action_tx.clone(),
            audit: Some(HealthAudit {
                device_id: device_id.into(),
                writer,
            }),
        })
    }

    /// Register or update the policy for an app.
    pub fn set_policy(&self, app_id: &str, policy: HealthPolicy) {
        let mut guard = self.apps.lock().unwrap();
        guard
            .entry(app_id.to_string())
            .and_modify(|h| h.policy = policy.clone())
            .or_insert_with(|| AppHealth {
                consecutive_failures: 0,
                last_status: HealthStatus::Unknown,
                last_restart: None,
                policy,
            });
    }

    /// Feed a new health report.  May fire a HealthAction via the channel.
    pub async fn report(&self, app_id: &str, status: HealthStatus) {
        let action_opt = {
            let mut guard = self.apps.lock().unwrap();
            let entry = guard
                .entry(app_id.to_string())
                .or_insert_with(|| AppHealth {
                    consecutive_failures: 0,
                    last_status: HealthStatus::Unknown,
                    last_restart: None,
                    policy: HealthPolicy::default(),
                });

            entry.last_status = status.clone();

            if status == HealthStatus::Unhealthy {
                entry.consecutive_failures += 1;
            } else {
                entry.consecutive_failures = 0;
            }

            if entry.consecutive_failures >= entry.policy.unhealthy_threshold {
                let now = Instant::now();
                let too_soon = entry
                    .last_restart
                    .map(|t| now.duration_since(t) < entry.policy.min_restart_interval)
                    .unwrap_or(false);

                if !too_soon {
                    if matches!(
                        entry.policy.action,
                        PolicyAction::Restart | PolicyAction::RestartThenAlert
                    ) {
                        entry.last_restart = Some(now);
                    }
                    Some(HealthAction {
                        app_id: app_id.to_string(),
                        action: entry.policy.action.clone(),
                        consecutive_failures: entry.consecutive_failures,
                    })
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(action) = action_opt {
            warn!(
                app_id = %action.app_id,
                failures = action.consecutive_failures,
                action = ?action.action,
                "Health policy triggered"
            );
            self.audit_policy_action(&action);
            let _ = self.action_tx.send(action).await;
        }
    }

    fn audit_policy_action(&self, action: &HealthAction) {
        if !matches!(
            action.action,
            PolicyAction::Restart | PolicyAction::RestartThenAlert
        ) {
            return;
        }

        let Some(audit) = self.audit.as_ref() else {
            return;
        };
        let principal = Principal::new("default", &audit.device_id, "LocalSystem", Role::Admin);
        let policy_name = format!("{:?}", action.action);
        let mut event = AuditEvent::new(
            format!("health-action-{}", uuid::Uuid::new_v4()),
            SystemTime::now(),
            principal,
            Action::Execute,
            Resource::AppControl,
            format!("app:{}:health-policy:{policy_name}", action.app_id),
            "restart_triggered",
            uuid::Uuid::new_v4().to_string(),
        );
        event.params_digest = digest_params(serde_json::json!({
            "app_id": action.app_id,
            "policy": policy_name,
            "consecutive_failures": action.consecutive_failures,
        }));
        let _ = audit.writer.write_entry(event);
    }

    pub fn consecutive_failures(&self, app_id: &str) -> u32 {
        self.apps
            .lock()
            .unwrap()
            .get(app_id)
            .map(|h| h.consecutive_failures)
            .unwrap_or(0)
    }

    pub fn last_status(&self, app_id: &str) -> HealthStatus {
        self.apps
            .lock()
            .unwrap()
            .get(app_id)
            .map(|h| h.last_status.clone())
            .unwrap_or(HealthStatus::Unknown)
    }
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use agent_core::chain::{AuditSink, AuditWriter};
    use agent_core::security::{Action, AuditEvent, Resource};
    use agent_store::{AuditEventFilter, StateStore};

    struct StoreAuditSink {
        store: Arc<Mutex<StateStore>>,
    }

    impl AuditSink for StoreAuditSink {
        fn append_audit_event(&self, event: AuditEvent) -> Result<(), String> {
            self.store
                .lock()
                .unwrap()
                .append_audit_event(event)
                .map_err(|error| error.to_string())
        }
    }

    #[tokio::test]
    async fn triggers_after_threshold() {
        let (tx, mut rx) = mpsc::channel(8);
        let eval = HealthEvaluator::new(tx);

        eval.set_policy(
            "app1",
            HealthPolicy {
                unhealthy_threshold: 2,
                action: PolicyAction::Restart,
                min_restart_interval: Duration::from_millis(0),
            },
        );

        eval.report("app1", HealthStatus::Unhealthy).await;
        assert!(rx.try_recv().is_err(), "no action yet after 1 failure");

        eval.report("app1", HealthStatus::Unhealthy).await;
        let action = rx.try_recv().expect("action after 2 failures");
        assert_eq!(action.app_id, "app1");
        assert_eq!(action.consecutive_failures, 2);
    }

    #[tokio::test]
    async fn healthy_report_resets_counter() {
        let (tx, _rx) = mpsc::channel(8);
        let eval = HealthEvaluator::new(tx);

        eval.report("app2", HealthStatus::Unhealthy).await;
        assert_eq!(eval.consecutive_failures("app2"), 1);

        eval.report("app2", HealthStatus::Healthy).await;
        assert_eq!(eval.consecutive_failures("app2"), 0);
    }

    #[tokio::test]
    async fn rate_limits_restarts() {
        let (tx, mut rx) = mpsc::channel(8);
        let eval = HealthEvaluator::new(tx);

        eval.set_policy(
            "app3",
            HealthPolicy {
                unhealthy_threshold: 1,
                action: PolicyAction::Restart,
                // Large interval — second trigger should be suppressed
                min_restart_interval: Duration::from_secs(9999),
            },
        );

        eval.report("app3", HealthStatus::Unhealthy).await;
        assert!(rx.try_recv().is_ok(), "first trigger fires");

        eval.report("app3", HealthStatus::Unhealthy).await;
        assert!(rx.try_recv().is_err(), "second suppressed by rate-limit");
    }

    #[tokio::test]
    async fn restart_policy_trigger_writes_local_system_audit() {
        let (tx, mut rx) = mpsc::channel(8);
        let store = Arc::new(Mutex::new(StateStore::open_in_memory().unwrap()));
        let audit_writer = AuditWriter::new(Arc::new(StoreAuditSink {
            store: Arc::clone(&store),
        }));
        let eval = HealthEvaluator::new(tx).with_audit("test-device", audit_writer);

        eval.set_policy(
            "app4",
            HealthPolicy {
                unhealthy_threshold: 1,
                action: PolicyAction::Restart,
                min_restart_interval: Duration::from_millis(0),
            },
        );

        eval.report("app4", HealthStatus::Unhealthy).await;
        let action = rx.try_recv().expect("restart action emitted");
        assert_eq!(action.app_id, "app4");
        tokio::task::yield_now().await;

        let events = store
            .lock()
            .unwrap()
            .query_audit_events(&AuditEventFilter {
                principal: Some("LocalSystem".into()),
                resource: Some(Resource::AppControl),
                action: Some(Action::Execute),
                result: Some("restart_triggered".into()),
                ..AuditEventFilter::default()
            })
            .unwrap();
        assert!(events.iter().any(|event| {
            event.target == "app:app4:health-policy:Restart" && !event.params_digest.is_empty()
        }));
    }
}
