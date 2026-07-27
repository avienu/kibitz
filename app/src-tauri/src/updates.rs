//! In-app update checking (src/lib.rs registers `update_check`).
//!
//! Thin wrapper over tauri-plugin-updater. Design constraints:
//!
//! - **Honesty first**: until a real minisign pubkey replaces the
//!   `TODO-UPDATER-PUBKEY` placeholder in tauri.conf.json, `update_check`
//!   returns `configured: false` *without touching the network*. The
//!   Settings row states this plainly instead of pretending to check.
//! - **User-initiated only**: this command runs when the user presses
//!   "Check now", or once at launch when the "Check for updates" setting
//!   (default ON, stored frontend-side) is enabled — see
//!   app/src/lib/updates.ts. Nothing polls in the background.
//! - Errors are folded into the payload (`error` field) rather than a
//!   rejected promise, so the Settings row can render every state.

use serde::Serialize;
use tauri_plugin_updater::UpdaterExt;

/// Result payload for a single update check.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    /// False while the updater pubkey is still the repo placeholder (or the
    /// plugin failed to construct); the UI says "not configured" honestly.
    pub configured: bool,
    /// True when a strictly newer version is available upstream.
    pub available: bool,
    /// The running app version (from tauri.conf.json).
    pub current: String,
    /// Version offered by the feed, when `available`.
    pub version: Option<String>,
    /// Release notes from the feed, when provided.
    pub notes: Option<String>,
    /// Human-readable reason when the check could not complete.
    pub error: Option<String>,
}

impl UpdateCheck {
    fn unconfigured(current: String, why: String) -> Self {
        Self {
            configured: false,
            available: false,
            current,
            version: None,
            notes: None,
            error: Some(why),
        }
    }
}

/// True when the configured pubkey is still the checked-in placeholder
/// (or empty). Kept as a pure function so it is unit-testable.
fn is_placeholder_pubkey(pubkey: &str) -> bool {
    let k = pubkey.trim();
    k.is_empty() || k.contains("TODO-UPDATER-PUBKEY")
}

/// Extract the updater pubkey string from the merged Tauri config, if any.
fn configured_pubkey(config: &tauri::Config) -> Option<String> {
    config
        .plugins
        .0
        .get("updater")
        .and_then(|v| v.get("pubkey"))
        .and_then(|v| v.as_str())
        .map(str::to_owned)
}

/// Check the release feed for a newer version. Never rejects for expected
/// states (unconfigured / offline): those come back in the payload.
#[tauri::command]
pub async fn update_check(app: tauri::AppHandle) -> Result<UpdateCheck, String> {
    let current = app.package_info().version.to_string();

    match configured_pubkey(app.config()) {
        Some(k) if !is_placeholder_pubkey(&k) => {}
        _ => {
            return Ok(UpdateCheck::unconfigured(
                current,
                "updater not configured: no signing pubkey yet (pre-release build)".into(),
            ));
        }
    }

    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => {
            return Ok(UpdateCheck::unconfigured(
                current,
                format!("updater not configured: {e}"),
            ));
        }
    };

    match updater.check().await {
        Ok(Some(update)) => Ok(UpdateCheck {
            configured: true,
            available: true,
            current,
            version: Some(update.version.clone()),
            notes: update.body.clone(),
            error: None,
        }),
        Ok(None) => Ok(UpdateCheck {
            configured: true,
            available: false,
            current,
            version: None,
            notes: None,
            error: None,
        }),
        Err(e) => Ok(UpdateCheck {
            configured: true,
            available: false,
            current,
            version: None,
            notes: None,
            error: Some(format!("update check failed: {e}")),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::is_placeholder_pubkey;

    #[test]
    fn placeholder_pubkey_is_detected() {
        assert!(is_placeholder_pubkey(""));
        assert!(is_placeholder_pubkey("   "));
        assert!(is_placeholder_pubkey(
            "TODO-UPDATER-PUBKEY: generate with `npm run tauri signer generate`"
        ));
    }

    #[test]
    fn real_looking_pubkey_is_not_placeholder() {
        // Shape of a minisign public key (base64 payload is illustrative).
        assert!(!is_placeholder_pubkey(
            "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEFCQ0RFRg=="
        ));
    }
}
