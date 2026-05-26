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
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ValidatedCommand {
    pub command_id: String,
    args: HashMap<String, String>,
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
                        kind: ParameterKind::String,
                        required: true,
                    }],
                },
            )]),
        }
    }
}

impl CommandPolicy {
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
                ParameterKind::String => {
                    let value = value.as_str().ok_or_else(|| {
                        CommandPolicyError::InvalidParameterType(parameter.name.clone())
                    })?;
                    if value.trim().is_empty() {
                        return Err(CommandPolicyError::MissingParameter(parameter.name.clone()));
                    }
                    args.insert(parameter.name.clone(), value.to_string());
                }
            }
        }

        Ok(ValidatedCommand {
            command_id: template.command_id.clone(),
            args,
        })
    }
}

#[derive(Debug, Clone)]
struct CommandTemplate {
    command_id: String,
    allowed_roles: HashSet<Role>,
    parameters: Vec<ParameterSpec>,
}

#[derive(Debug, Clone)]
struct ParameterSpec {
    name: String,
    kind: ParameterKind,
    required: bool,
}

#[derive(Debug, Clone, Copy)]
enum ParameterKind {
    String,
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
}
