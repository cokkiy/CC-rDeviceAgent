use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::security::{Principal, Role};

pub type CommandPolicyResult<T> = Result<T, CommandPolicyError>;

#[derive(Debug, Clone, Eq, PartialEq, thiserror::Error)]
pub enum CommandPolicyError {
    #[error("unknown command")]
    UnknownCommand,
    #[error("role is not allowed to execute command")]
    RoleNotAllowed,
    #[error("missing required parameter: {0}")]
    MissingParameter(String),
    #[error("invalid parameter type for: {0}")]
    InvalidParameterType(String),
    #[error("invalid parameter value for: {0}")]
    InvalidParameterValue(String),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ValidatedCommand {
    pub command_id: String,
    args: HashMap<String, String>,
    pub timeout_ms: u64,
    pub resource_limits: Option<CommandResourceLimits>,
    pub sandbox_profile: Option<String>,
}

impl ValidatedCommand {
    pub fn argument(&self, name: &str) -> Option<&str> {
        self.args.get(name).map(String::as_str)
    }
}

#[derive(Debug, Clone)]
pub struct CommandPolicy {
    commands: HashMap<String, CommandTemplate>,
}

impl Default for CommandPolicy {
    fn default() -> Self {
        Self {
            commands: HashMap::from([(
                "restart_process".to_string(),
                CommandTemplate {
                    command_id: "restart_process".to_string(),
                    allowed_roles: HashSet::from([Role::Admin, Role::Operator]),
                    parameters: vec![ParameterSpec {
                        name: "process_name".to_string(),
                        kind: ParameterKind::String {
                            max_len: 128,
                            allow_shell_metacharacters: false,
                        },
                        required: true,
                    }],
                    timeout_ms: 30_000,
                    resource_limits: Some(CommandResourceLimits {
                        memory_bytes: None,
                        cpu_millis: Some(10_000),
                        open_files: Some(64),
                    }),
                    sandbox_profile: Some("process-control".to_string()),
                },
            )]),
        }
    }
}

impl CommandPolicy {
    pub fn from_templates(templates: Vec<CommandTemplate>) -> Self {
        Self {
            commands: templates
                .into_iter()
                .map(|template| (template.command_id.clone(), template))
                .collect(),
        }
    }

    pub fn validate(
        &self,
        principal: &Principal,
        command_id: &str,
        params: &Value,
    ) -> CommandPolicyResult<ValidatedCommand> {
        let template = self
            .commands
            .get(command_id)
            .ok_or(CommandPolicyError::UnknownCommand)?;
        if !template.allowed_roles.contains(&principal.role) {
            return Err(CommandPolicyError::RoleNotAllowed);
        }

        let mut args = HashMap::new();
        for parameter in &template.parameters {
            let Some(value) = params.get(&parameter.name) else {
                if parameter.required {
                    return Err(CommandPolicyError::MissingParameter(parameter.name.clone()));
                }
                continue;
            };

            match parameter.kind {
                ParameterKind::String {
                    max_len,
                    allow_shell_metacharacters,
                } => {
                    let value = value.as_str().ok_or_else(|| {
                        CommandPolicyError::InvalidParameterType(parameter.name.clone())
                    })?;
                    if value.trim().is_empty() {
                        return Err(CommandPolicyError::MissingParameter(parameter.name.clone()));
                    }
                    if value.len() > max_len
                        || (!allow_shell_metacharacters && contains_shell_metacharacter(value))
                    {
                        return Err(CommandPolicyError::InvalidParameterValue(
                            parameter.name.clone(),
                        ));
                    }
                    args.insert(parameter.name.clone(), value.to_string());
                }
            }
        }

        Ok(ValidatedCommand {
            command_id: template.command_id.clone(),
            args,
            timeout_ms: template.timeout_ms,
            resource_limits: template.resource_limits.clone(),
            sandbox_profile: template.sandbox_profile.clone(),
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CommandTemplate {
    pub command_id: String,
    pub allowed_roles: HashSet<Role>,
    pub parameters: Vec<ParameterSpec>,
    pub timeout_ms: u64,
    pub resource_limits: Option<CommandResourceLimits>,
    pub sandbox_profile: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ParameterSpec {
    pub name: String,
    pub kind: ParameterKind,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ParameterKind {
    String {
        max_len: usize,
        allow_shell_metacharacters: bool,
    },
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CommandResourceLimits {
    pub memory_bytes: Option<u64>,
    pub cpu_millis: Option<u64>,
    pub open_files: Option<u64>,
}

fn contains_shell_metacharacter(value: &str) -> bool {
    value
        .chars()
        .any(|ch| matches!(ch, ';' | '&' | '|' | '`' | '$' | '<' | '>' | '\n' | '\r'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::{Principal, Role};
    use serde_json::json;

    #[test]
    fn rejects_unknown_command_id() {
        let policy = CommandPolicy::default();
        let principal = Principal::new("tenant-a", "device-1", "operator-user", Role::Operator);

        assert_eq!(
            policy.validate(&principal, "unknown", &json!({})),
            Err(CommandPolicyError::UnknownCommand)
        );
    }

    #[test]
    fn rejects_role_without_command_permission() {
        let policy = CommandPolicy::default();
        let principal = Principal::new("tenant-a", "device-1", "readonly-user", Role::Readonly);

        assert_eq!(
            policy.validate(
                &principal,
                "restart_process",
                &json!({ "process_name": "nginx" })
            ),
            Err(CommandPolicyError::RoleNotAllowed)
        );
    }

    #[test]
    fn rejects_missing_required_parameter() {
        let policy = CommandPolicy::default();
        let principal = Principal::new("tenant-a", "device-1", "operator-user", Role::Operator);

        assert_eq!(
            policy.validate(&principal, "restart_process", &json!({})),
            Err(CommandPolicyError::MissingParameter(
                "process_name".to_string()
            ))
        );
    }

    #[test]
    fn accepts_whitelisted_command_with_valid_parameters() {
        let policy = CommandPolicy::default();
        let principal = Principal::new("tenant-a", "device-1", "operator-user", Role::Operator);
        let command = policy
            .validate(
                &principal,
                "restart_process",
                &json!({ "process_name": "nginx" }),
            )
            .unwrap();

        assert_eq!(command.command_id, "restart_process");
        assert_eq!(command.argument("process_name"), Some("nginx"));
    }

    #[test]
    fn rejects_shell_metacharacters_and_overlong_parameters() {
        let policy = CommandPolicy::default();
        let principal = Principal::new("tenant-a", "device-1", "operator-user", Role::Operator);

        assert_eq!(
            policy.validate(
                &principal,
                "restart_process",
                &json!({ "process_name": "nginx; rm -rf /" })
            ),
            Err(CommandPolicyError::InvalidParameterValue(
                "process_name".to_string()
            ))
        );
        assert_eq!(
            policy.validate(
                &principal,
                "restart_process",
                &json!({ "process_name": "a".repeat(129) })
            ),
            Err(CommandPolicyError::InvalidParameterValue(
                "process_name".to_string()
            ))
        );
    }

    #[test]
    fn accepts_configured_command_template_with_runtime_controls() {
        let policy = CommandPolicy::from_templates(vec![CommandTemplate {
            command_id: "collect_logs".to_string(),
            allowed_roles: HashSet::from([Role::Admin]),
            parameters: vec![ParameterSpec {
                name: "profile".to_string(),
                kind: ParameterKind::String {
                    max_len: 32,
                    allow_shell_metacharacters: false,
                },
                required: true,
            }],
            timeout_ms: 5_000,
            resource_limits: Some(CommandResourceLimits {
                memory_bytes: Some(64 * 1024 * 1024),
                cpu_millis: Some(1_000),
                open_files: Some(32),
            }),
            sandbox_profile: Some("read-only-diagnostics".to_string()),
        }]);
        let principal = Principal::new("tenant-a", "device-1", "admin-user", Role::Admin);
        let command = policy
            .validate(&principal, "collect_logs", &json!({ "profile": "default" }))
            .unwrap();

        assert_eq!(command.timeout_ms, 5_000);
        assert_eq!(
            command.sandbox_profile.as_deref(),
            Some("read-only-diagnostics")
        );
        assert_eq!(command.resource_limits.unwrap().open_files, Some(32));
    }
}
