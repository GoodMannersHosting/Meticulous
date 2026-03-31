//! Non-secret JSON carried on job dispatch for controller-side resolution.

use indexmap::IndexMap;
use met_parser::SecretRef;
use serde::{Deserialize, Serialize};

/// Versioned payload in `JobDispatch.secret_resolution_hints_json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretResolutionHints {
    pub version: u32,
    pub refs: Vec<SecretResolutionRefHint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretResolutionRefHint {
    pub env_name: String,
    /// Row `path` / logical name in `builtin_secrets`.
    pub path: String,
}

/// Build JSON hints for stored/builtin refs only. Returns `(requires_exchange, json)`.
pub fn hints_json_from_secret_refs(refs: &IndexMap<String, SecretRef>) -> (bool, String) {
    let mut v = Vec::new();
    for (env_name, pref) in refs {
        match pref {
            SecretRef::Stored { name } | SecretRef::Builtin { name } => {
                v.push(SecretResolutionRefHint {
                    env_name: env_name.clone(),
                    path: name.clone(),
                });
            }
            SecretRef::Aws { .. } | SecretRef::Vault { .. } => {}
        }
    }
    if v.is_empty() {
        return (false, String::new());
    }
    let hints = SecretResolutionHints { version: 1, refs: v };
    (
        true,
        serde_json::to_string(&hints).unwrap_or_else(|_| "{}".to_string()),
    )
}
