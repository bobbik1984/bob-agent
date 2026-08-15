use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::{AppHandle, Manager};

use crate::db::DbState;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CapabilityState {
    Available,
    Degraded,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Capability {
    pub id: String,
    pub state: CapabilityState,
    pub reason_code: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CapabilitySnapshot {
    pub platform: String,
    pub request_channel: String,
    pub file_scope: String,
    pub connected_pc: bool,
    pub capabilities: Vec<Capability>,
}

impl CapabilitySnapshot {
    pub(crate) fn capture(
        app: &AppHandle,
        request_from_mobile: bool,
        global_file_access: bool,
    ) -> Self {
        let connected_pc = app
            .try_state::<DbState>()
            .and_then(|state| state.0.lock().ok().map(|conn| has_connected_pc(&conn)))
            .unwrap_or(false);
        Self::detect(
            std::env::consts::OS,
            request_from_mobile,
            global_file_access,
            connected_pc,
        )
    }

    fn detect(
        platform: &str,
        request_from_mobile: bool,
        global_file_access: bool,
        connected_pc: bool,
    ) -> Self {
        let mobile_runtime = matches!(platform, "android" | "ios");
        let mut capabilities = vec![
            available("calendar"),
            available("knowledge"),
            available("web"),
            available("sandbox_files"),
        ];
        for (id, desktop_only) in [
            ("desktop_browser", true),
            ("document_export", true),
            ("scheduler", true),
        ] {
            capabilities.push(if desktop_only && mobile_runtime {
                unavailable(id, "capability.platform_unsupported")
            } else {
                available(id)
            });
        }
        capabilities.push(if mobile_runtime && connected_pc {
            available("pc_handoff")
        } else if mobile_runtime {
            unavailable("pc_handoff", "capability.pc_not_connected")
        } else {
            unavailable("pc_handoff", "capability.not_needed_on_desktop")
        });
        capabilities.push(detected_without_adapter(
            "powershell",
            platform == "windows"
                && (command_exists("powershell.exe") || command_exists("pwsh.exe")),
            platform == "windows",
        ));
        capabilities.push(detected_without_adapter(
            "git",
            command_exists(if platform == "windows" {
                "git.exe"
            } else {
                "git"
            }),
            !mobile_runtime,
        ));

        Self {
            platform: platform.into(),
            request_channel: if request_from_mobile {
                "mobile".into()
            } else {
                "local_ui".into()
            },
            file_scope: if global_file_access && !mobile_runtime {
                "global_authorized".into()
            } else {
                "sandbox_and_authorized_folders".into()
            },
            connected_pc,
            capabilities,
        }
    }

    pub(crate) fn is_mobile_runtime(&self) -> bool {
        matches!(self.platform.as_str(), "android" | "ios")
    }

    pub(crate) fn mcp_runtime_available(&self) -> bool {
        !self.is_mobile_runtime()
    }

    pub(crate) fn capability(&self, id: &str) -> Option<&Capability> {
        self.capabilities.iter().find(|item| item.id == id)
    }

    pub(crate) fn tool_available(&self, name: &str) -> bool {
        match name {
            "enable_browser" | "browse_page" => !self.is_mobile_runtime(),
            "export_html" | "export_xlsx" | "export_docx" | "export_pptx" => {
                !self.is_mobile_runtime()
            }
            "list_cron_jobs" | "add_cron_job" | "remove_cron_job" | "toggle_cron_job" => {
                !self.is_mobile_runtime()
            }
            "send_to_pc_agent" => self.is_mobile_runtime() && self.connected_pc,
            "share_file" | "send_wechat_file" | "install_skill_from_url" => {
                !self.is_mobile_runtime()
            }
            _ => true,
        }
    }

    pub(crate) fn render_prompt(&self) -> String {
        let available = self
            .capabilities
            .iter()
            .filter(|item| item.state == CapabilityState::Available)
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let degraded = self
            .capabilities
            .iter()
            .filter(|item| item.state == CapabilityState::Degraded)
            .map(|item| format!("{} ({})", item.id, item.reason_code))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "\n## Runtime capability snapshot\nPlatform: {}\nRequest channel: {}\nFile scope: {}\nAvailable adapters: {}\nDetected but not callable: {}\nOnly tools exposed in this request are executable. Never infer an adapter from installed software alone.\n",
            self.platform,
            self.request_channel,
            self.file_scope,
            if available.is_empty() { "none" } else { &available },
            if degraded.is_empty() { "none" } else { &degraded },
        )
    }
}

fn has_connected_pc(conn: &rusqlite::Connection) -> bool {
    conn.query_row(
        "SELECT 1 FROM connected_devices WHERE platform IN ('windows', 'mac', 'linux', 'pc') ORDER BY last_seen DESC LIMIT 1",
        [],
        |_| Ok(()),
    )
    .is_ok()
}

fn command_exists(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|directory| Path::new(&directory).join(name).is_file())
    })
}

fn available(id: &str) -> Capability {
    Capability {
        id: id.into(),
        state: CapabilityState::Available,
        reason_code: "capability.adapter_ready".into(),
    }
}

fn unavailable(id: &str, reason_code: &str) -> Capability {
    Capability {
        id: id.into(),
        state: CapabilityState::Unavailable,
        reason_code: reason_code.into(),
    }
}

fn detected_without_adapter(id: &str, detected: bool, platform_supported: bool) -> Capability {
    if detected {
        Capability {
            id: id.into(),
            state: CapabilityState::Degraded,
            reason_code: "capability.adapter_missing".into(),
        }
    } else if platform_supported {
        unavailable(id, "capability.not_detected")
    } else {
        unavailable(id, "capability.platform_unsupported")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobile_runtime_hides_desktop_only_tools_without_a_pc() {
        let snapshot = CapabilitySnapshot::detect("android", true, false, false);
        assert!(snapshot.tool_available("web_search"));
        assert!(snapshot.tool_available("write_file"));
        assert!(!snapshot.tool_available("browse_page"));
        assert!(!snapshot.tool_available("export_docx"));
        assert!(!snapshot.tool_available("send_to_pc_agent"));
        assert_eq!(snapshot.file_scope, "sandbox_and_authorized_folders");
    }

    #[test]
    fn mobile_runtime_exposes_handoff_only_when_a_pc_is_connected() {
        let snapshot = CapabilitySnapshot::detect("ios", true, false, true);
        assert!(snapshot.tool_available("send_to_pc_agent"));
    }

    #[test]
    fn installed_commands_are_not_treated_as_callable_adapters() {
        let capability = detected_without_adapter("powershell", true, true);
        assert_eq!(capability.state, CapabilityState::Degraded);
        assert_eq!(capability.reason_code, "capability.adapter_missing");
    }
}
