// SessDeclPlugin: declarative login-session state
// Random pick #1 — focus: systemd-logind sessions
//
// Goal: declare whether a matching session should be present or absent.
// Query: prefers systemd D‑Bus (TODO), falls back to parsing `loginctl list-sessions`.
// Enforce: if desired.present == false and a matching live session exists, terminate it.
//          If desired.present == true, we only observe (can't force a login).
//
// Integrates with op_state::{StatePlugin, StateDiff, StateAction, ApplyResult, Checkpoint, PluginCapabilities}.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::process::Command;

use op_state::{ApplyResult, Checkpoint, PluginCapabilities, StateAction, StateDiff, StatePlugin};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessDecl {
    pub version: u32,
    pub items: Vec<LoginItem>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EnforcementMode {
    Enforce,
    ObserveOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginItem {
    pub id: String,
    pub mode: EnforcementMode,
    pub selector: Selector,
    pub desired: Desired,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Selector {
    pub user: Option<String>,
    pub tty: Option<String>,  // e.g. tty1 pts/0
    pub seat: Option<String>, // e.g. seat0
    pub kind: Option<String>, // e.g. "tty", "x11", "wayland" (best-effort)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Desired {
    pub present: bool,
}

#[derive(Debug, Clone)]
struct LiveSession {
    session_id: String,
    uid: String,
    user: String,
    seat: String,
    tty: String,
}

pub struct SessDeclPlugin;

impl Default for SessDeclPlugin {
    fn default() -> Self {
        Self
    }
}

impl SessDeclPlugin {
    pub fn new() -> Self {
        Self
    }

    fn list_sessions_fallback() -> Vec<LiveSession> {
        // Parse: loginctl list-sessions  (no JSON in older distros; parse columns)
        // Typical header:
        // SESSION  UID USER   SEAT  TTY
        //     2   1000 jeremy seat0 tty1
        let out = Command::new("loginctl")
            .arg("list-sessions")
            .arg("--no-legend")
            .arg("--no-pager")
            .output();

        let Ok(o) = out else { return vec![] };
        if !o.status.success() {
            return vec![];
        }
        let s = String::from_utf8_lossy(&o.stdout);
        let mut res = Vec::new();
        for line in s.lines() {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() >= 5 {
                res.push(LiveSession {
                    session_id: cols[0].to_string(),
                    uid: cols[1].to_string(),
                    user: cols[2].to_string(),
                    seat: cols[3].to_string(),
                    tty: cols[4].to_string(),
                });
            }
        }
        res
    }

    fn match_selector(sel: &Selector, sess: &LiveSession) -> bool {
        if let Some(u) = &sel.user {
            if &sess.user != u {
                return false;
            }
        }
        if let Some(t) = &sel.tty {
            if &sess.tty != t {
                return false;
            }
        }
        if let Some(s) = &sel.seat {
            if &sess.seat != s {
                return false;
            }
        }
        // kind not resolved in fallback
        true
    }

    fn terminate_session(id: &str) -> Result<()> {
        let st = Command::new("loginctl")
            .arg("terminate-session")
            .arg(id)
            .status()?;
        if !st.success() {
            anyhow::bail!("loginctl terminate-session {} failed", id);
        }
        Ok(())
    }
}

#[async_trait]
impl StatePlugin for SessDeclPlugin {
    fn name(&self) -> &str {
        "sess"
    }
    fn version(&self) -> &str {
        "1.0.0"
    }

    async fn query_current_state(&self) -> Result<Value> {
        // Returns current session inventory in a simple shape
        let sessions = Self::list_sessions_fallback();
        let vec: Vec<Value> = sessions
            .into_iter()
            .map(|s| {
                serde_json::json!({
                    "session_id": s.session_id,
                    "uid": s.uid,
                    "user": s.user,
                    "seat": s.seat,
                    "tty": s.tty,
                })
            })
            .collect();
        Ok(serde_json::json!({"sessions": vec}))
    }

    async fn calculate_diff(&self, _current: &Value, desired: &Value) -> Result<StateDiff> {
        let decl: SessDecl =
            serde_json::from_value(desired.clone()).context("desired must be SessDecl")?;

        // Snapshot live state for diff
        let live = Self::list_sessions_fallback();

        let mut actions = Vec::new();
        for item in &decl.items {
            let matches: Vec<&LiveSession> = live
                .iter()
                .filter(|s| Self::match_selector(&item.selector, s))
                .collect();
            let is_present = !matches.is_empty();

            match (item.mode, item.desired.present, is_present) {
                (EnforcementMode::ObserveOnly, _, _) => {
                    // Observe-only mode: no action taken
                    actions.push(StateAction::NoOp {
                        resource: item.id.clone(),
                    });
                }
                (EnforcementMode::Enforce, true, true) => {
                    // Desired session is present: no action needed
                    actions.push(StateAction::NoOp {
                        resource: item.id.clone(),
                    });
                }
                (EnforcementMode::Enforce, true, false) => {
                    // Cannot force login; mark as NoOp
                    actions.push(StateAction::NoOp {
                        resource: item.id.clone(),
                    });
                }
                (EnforcementMode::Enforce, false, true) => {
                    // Session should be absent but is present: terminate it
                    actions.push(StateAction::Modify {
                        resource: item.id.clone(),
                        changes: serde_json::to_value(item)?,
                    });
                }
                (EnforcementMode::Enforce, false, false) => {
                    // Session already absent: no action needed
                    actions.push(StateAction::NoOp {
                        resource: item.id.clone(),
                    });
                }
            }
        }

        Ok(StateDiff {
            plugin: self.name().to_string(),
            actions,
            metadata: op_state::DiffMetadata {
                timestamp: chrono::Utc::now().timestamp(),
                current_hash: format!("{:x}", md5::compute("sess-current")),
                desired_hash: format!("{:x}", md5::compute(serde_json::to_string(&desired)?)),
            },
        })
    }

    async fn apply_state(&self, diff: &StateDiff) -> Result<ApplyResult> {
        let mut changes_applied = Vec::new();
        let mut errors = Vec::new();

        // Fresh snapshot for enforcement
        let live = Self::list_sessions_fallback();

        for action in &diff.actions {
            match action {
                StateAction::Modify { resource, changes } => {
                    let item: LoginItem = serde_json::from_value(changes.clone())?;
                    // Find matching sessions and terminate all
                    let matches: Vec<&LiveSession> = live
                        .iter()
                        .filter(|s| Self::match_selector(&item.selector, s))
                        .collect();
                    if matches.is_empty() {
                        changes_applied.push(format!("{}: already absent", resource));
                        continue;
                    }
                    for s in matches {
                        match Self::terminate_session(&s.session_id) {
                            Ok(_) => changes_applied
                                .push(format!("{}: terminated session {}", resource, s.session_id)),
                            Err(e) => errors.push(format!("{}: {}", resource, e)),
                        }
                    }
                }
                StateAction::NoOp { resource } => {
                    changes_applied.push(format!("{}: no action required", resource));
                }
                _ => {} // Create/Delete not used here
            }
        }

        Ok(ApplyResult {
            success: errors.is_empty(),
            changes_applied,
            errors,
            checkpoint: None,
        })
    }

    async fn verify_state(&self, desired: &Value) -> Result<bool> {
        let decl: SessDecl = serde_json::from_value(desired.clone())?;
        let live = Self::list_sessions_fallback();
        for item in &decl.items {
            let is_present = live.iter().any(|s| Self::match_selector(&item.selector, s));
            if item.desired.present != is_present {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn create_checkpoint(&self) -> Result<Checkpoint> {
        Ok(Checkpoint {
            id: format!("sess-{}", chrono::Utc::now().timestamp()),
            plugin: self.name().to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            state_snapshot: serde_json::json!({ "note": "login sessions are ephemeral; checkpoint is advisory" }),
            backend_checkpoint: None,
        })
    }

    async fn rollback(&self, _checkpoint: &Checkpoint) -> Result<()> {
        // No rollback semantics for sessions
        Ok(())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities {
            supports_rollback: false,
            supports_checkpoints: true,
            supports_verification: true,
            atomic_operations: false,
        }
    }
}
