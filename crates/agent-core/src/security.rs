use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime};

use ring::digest;
use serde::{Deserialize, Serialize};

pub type SecurityResult<T> = Result<T, SecurityError>;

#[derive(Debug, Clone, Eq, PartialEq, thiserror::Error)]
pub enum SecurityError {
    #[error("replay detected")]
    ReplayDetected,
    #[error("timestamp is outside the allowed replay window")]
    TimestampOutOfWindow,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Principal {
    pub tenant_id: String,
    pub device_id: String,
    pub subject: String,
    pub role: Role,
}

impl Principal {
    pub fn new(
        tenant_id: impl Into<String>,
        device_id: impl Into<String>,
        subject: impl Into<String>,
        role: Role,
    ) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            device_id: device_id.into(),
            subject: subject.into(),
            role,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum Role {
    Admin,
    Operator,
    Readonly,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum Resource {
    Telemetry,
    ControlCommand,
    FileTransfer,
    Configuration,
    Upgrade,
    AppControl,
    SecurityPolicy,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum Action {
    Read,
    Execute,
    Write,
    Manage,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum Decision {
    Allow,
    Deny,
}

#[derive(Debug, Clone)]
pub struct RbacPolicy {
    grants: HashMap<Role, HashSet<(Resource, Action)>>,
}

impl Default for RbacPolicy {
    fn default() -> Self {
        let mut grants = HashMap::new();

        grants.insert(
            Role::Admin,
            HashSet::from([
                (Resource::Telemetry, Action::Read),
                (Resource::ControlCommand, Action::Execute),
                (Resource::FileTransfer, Action::Read),
                (Resource::FileTransfer, Action::Write),
                (Resource::Configuration, Action::Read),
                (Resource::Configuration, Action::Write),
                (Resource::Upgrade, Action::Execute),
                (Resource::AppControl, Action::Execute),
                (Resource::SecurityPolicy, Action::Manage),
            ]),
        );
        grants.insert(
            Role::Operator,
            HashSet::from([
                (Resource::Telemetry, Action::Read),
                (Resource::ControlCommand, Action::Execute),
                (Resource::FileTransfer, Action::Read),
                (Resource::FileTransfer, Action::Write),
                (Resource::Configuration, Action::Read),
                (Resource::Upgrade, Action::Execute),
                (Resource::AppControl, Action::Execute),
            ]),
        );
        grants.insert(
            Role::Readonly,
            HashSet::from([
                (Resource::Telemetry, Action::Read),
                (Resource::FileTransfer, Action::Read),
                (Resource::Configuration, Action::Read),
            ]),
        );

        Self { grants }
    }
}

impl RbacPolicy {
    pub fn authorize(&self, principal: &Principal, resource: Resource, action: Action) -> Decision {
        match self.grants.get(&principal.role) {
            Some(grants) if grants.contains(&(resource, action)) => Decision::Allow,
            _ => Decision::Deny,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SecurityRequest {
    pub principal: Principal,
    pub resource: Resource,
    pub action: Action,
    pub timestamp: SystemTime,
    pub nonce: String,
}

impl SecurityRequest {
    pub fn new(
        principal: Principal,
        resource: Resource,
        action: Action,
        timestamp: SystemTime,
        nonce: impl Into<String>,
    ) -> Self {
        Self {
            principal,
            resource,
            action,
            timestamp,
            nonce: nonce.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BasicSecurityCenter {
    policy: RbacPolicy,
    replay_guard: ReplayGuard,
}

impl BasicSecurityCenter {
    pub fn new(policy: RbacPolicy, replay_guard: ReplayGuard) -> Self {
        Self {
            policy,
            replay_guard,
        }
    }

    pub fn authorize_request(
        &mut self,
        request: &SecurityRequest,
        now: SystemTime,
    ) -> SecurityResult<Decision> {
        let decision = self
            .policy
            .authorize(&request.principal, request.resource, request.action);
        if decision == Decision::Deny {
            return Ok(Decision::Deny);
        }

        self.replay_guard.check(
            &request.principal,
            request.action,
            request.timestamp,
            &request.nonce,
            now,
        )?;

        Ok(Decision::Allow)
    }
}

#[derive(Debug, Clone)]
pub struct ReplayGuard {
    allowed_skew: Duration,
    seen: HashMap<ReplayKey, SystemTime>,
}

impl ReplayGuard {
    pub fn new(allowed_skew: Duration) -> Self {
        Self {
            allowed_skew,
            seen: HashMap::new(),
        }
    }

    pub fn check(
        &mut self,
        principal: &Principal,
        action: Action,
        timestamp: SystemTime,
        nonce: &str,
        now: SystemTime,
    ) -> SecurityResult<()> {
        let delta = if timestamp >= now {
            timestamp.duration_since(now)
        } else {
            now.duration_since(timestamp)
        }
        .map_err(|_| SecurityError::TimestampOutOfWindow)?;

        if delta > self.allowed_skew {
            return Err(SecurityError::TimestampOutOfWindow);
        }

        self.prune(now);

        let key = ReplayKey {
            tenant_id: principal.tenant_id.clone(),
            device_id: principal.device_id.clone(),
            subject: principal.subject.clone(),
            action,
            nonce: nonce.to_string(),
        };

        if self.seen.contains_key(&key) {
            return Err(SecurityError::ReplayDetected);
        }

        self.seen.insert(key, timestamp);
        Ok(())
    }

    fn prune(&mut self, now: SystemTime) {
        let allowed_skew = self.allowed_skew;
        self.seen.retain(|_, timestamp| {
            now.duration_since(*timestamp)
                .map(|age| age <= allowed_skew)
                .unwrap_or(true)
        });
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
struct ReplayKey {
    tenant_id: String,
    device_id: String,
    subject: String,
    action: Action,
    nonce: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub event_id: String,
    pub timestamp: SystemTime,
    pub tenant_id: String,
    pub device_id: String,
    pub principal: String,
    pub action: Action,
    pub resource: Resource,
    pub target: String,
    pub params_digest: String,
    pub result: String,
    pub trace_id: String,
    pub prev_hash: String,
    pub hash: String,
}

impl AuditEvent {
    pub fn new(
        event_id: impl Into<String>,
        timestamp: SystemTime,
        principal: Principal,
        action: Action,
        resource: Resource,
        target: impl Into<String>,
        result: impl Into<String>,
        trace_id: impl Into<String>,
    ) -> Self {
        Self {
            event_id: event_id.into(),
            timestamp,
            tenant_id: principal.tenant_id,
            device_id: principal.device_id,
            principal: principal.subject,
            action,
            resource,
            target: target.into(),
            params_digest: String::new(),
            result: result.into(),
            trace_id: trace_id.into(),
            prev_hash: String::new(),
            hash: String::new(),
        }
    }

    pub fn calculate_hash(&self, prev_hash: &str) -> String {
        let timestamp_ms = self
            .timestamp
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        let input = format!(
            "{}\n{}\n{}\n{}\n{}\n{:?}\n{:?}\n{}\n{}\n{}\n{}",
            prev_hash,
            self.event_id,
            timestamp_ms,
            self.tenant_id,
            self.device_id,
            self.action,
            self.resource,
            self.target,
            self.params_digest,
            self.result,
            self.trace_id
        );
        hex_encode(digest::digest(&digest::SHA256, input.as_bytes()).as_ref())
    }
}

#[derive(Debug, Clone, Default)]
pub struct AuditChain {
    events: Vec<AuditEvent>,
}

impl AuditChain {
    pub fn append(&mut self, mut event: AuditEvent) {
        let prev_hash = self
            .events
            .last()
            .map(|event| event.hash.clone())
            .unwrap_or_default();
        event.prev_hash = prev_hash;
        event.hash = event.calculate_hash(&event.prev_hash);
        self.events.push(event);
    }

    pub fn verify(&self) -> bool {
        let mut expected_prev_hash = String::new();
        for event in &self.events {
            if event.prev_hash != expected_prev_hash {
                return false;
            }
            if event.hash != event.calculate_hash(&event.prev_hash) {
                return false;
            }
            expected_prev_hash = event.hash.clone();
        }
        true
    }

    pub fn events(&self) -> &[AuditEvent] {
        &self.events
    }

    pub fn events_mut(&mut self) -> &mut [AuditEvent] {
        &mut self.events
    }

    pub fn push_stored(&mut self, event: AuditEvent) {
        self.events.push(event);
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn readonly_principal() -> Principal {
        Principal::new("tenant-a", "device-1", "readonly-user", Role::Readonly)
    }

    #[test]
    fn readonly_role_can_read_but_cannot_control() {
        let policy = RbacPolicy::default();
        let principal = readonly_principal();

        assert_eq!(
            policy.authorize(&principal, Resource::Telemetry, Action::Read),
            Decision::Allow
        );
        assert_eq!(
            policy.authorize(&principal, Resource::ControlCommand, Action::Execute),
            Decision::Deny
        );
    }

    #[test]
    fn operator_can_execute_control_but_cannot_manage_security_policy() {
        let policy = RbacPolicy::default();
        let principal = Principal::new("tenant-a", "device-1", "operator-user", Role::Operator);

        assert_eq!(
            policy.authorize(&principal, Resource::ControlCommand, Action::Execute),
            Decision::Allow
        );
        assert_eq!(
            policy.authorize(&principal, Resource::SecurityPolicy, Action::Manage),
            Decision::Deny
        );
    }

    #[test]
    fn replay_guard_rejects_reused_nonce_for_same_principal_and_action() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        let mut guard = ReplayGuard::new(Duration::from_secs(300));
        let principal = readonly_principal();

        assert_eq!(
            guard.check(&principal, Action::Read, now, "nonce-1", now),
            Ok(())
        );
        assert_eq!(
            guard.check(&principal, Action::Read, now, "nonce-1", now),
            Err(SecurityError::ReplayDetected)
        );
    }

    #[test]
    fn replay_guard_rejects_timestamps_outside_allowed_window() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        let mut guard = ReplayGuard::new(Duration::from_secs(300));
        let principal = readonly_principal();

        assert_eq!(
            guard.check(
                &principal,
                Action::Read,
                now - Duration::from_secs(301),
                "nonce-1",
                now
            ),
            Err(SecurityError::TimestampOutOfWindow)
        );
    }

    #[test]
    fn security_center_authorizes_allowed_request_and_rejects_replay() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        let mut center = BasicSecurityCenter::new(
            RbacPolicy::default(),
            ReplayGuard::new(Duration::from_secs(300)),
        );
        let principal = Principal::new("tenant-a", "device-1", "operator-user", Role::Operator);
        let request = SecurityRequest::new(
            principal,
            Resource::ControlCommand,
            Action::Execute,
            now,
            "nonce-1",
        );

        assert_eq!(center.authorize_request(&request, now), Ok(Decision::Allow));
        assert_eq!(
            center.authorize_request(&request, now),
            Err(SecurityError::ReplayDetected)
        );
    }

    #[test]
    fn security_center_denies_unauthorized_request_without_consuming_nonce() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        let mut center = BasicSecurityCenter::new(
            RbacPolicy::default(),
            ReplayGuard::new(Duration::from_secs(300)),
        );
        let request = SecurityRequest::new(
            readonly_principal(),
            Resource::ControlCommand,
            Action::Execute,
            now,
            "nonce-1",
        );

        assert_eq!(center.authorize_request(&request, now), Ok(Decision::Deny));
        assert_eq!(center.authorize_request(&request, now), Ok(Decision::Deny));
    }

    #[test]
    fn audit_chain_links_events_and_detects_tampering() {
        let principal = Principal::new("tenant-a", "device-1", "operator-user", Role::Operator);
        let first = AuditEvent::new(
            "event-1",
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_000),
            principal.clone(),
            Action::Execute,
            Resource::ControlCommand,
            "process:nginx",
            "success",
            "trace-1",
        );
        let second = AuditEvent::new(
            "event-2",
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_001),
            principal,
            Action::Read,
            Resource::Telemetry,
            "telemetry:cpu",
            "success",
            "trace-1",
        );

        let mut chain = AuditChain::default();
        chain.append(first);
        chain.append(second);

        assert!(chain.verify());
        chain.events_mut()[0].result = "failed".to_string();
        assert!(!chain.verify());
    }
}
