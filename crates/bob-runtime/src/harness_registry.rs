//! Unified Harness Registry & Generic ACP Adapter Metadata.
//!
//! Single source of truth for built-in native harnesses, standard ACP-over-stdio
//! CLI tools (AGY, Copilot, Kimi Code, Qoder, ZCode, WorkBuddy, Goose), and
//! discoverable ecosystem CLIs (Doubao/Ark CLI, Hermes Agent, OpenClaw).
//! Also supports loading optional locally-trusted custom ACP entries from
//! `<data_dir>/acp_registry.json` without modifying Rust code.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessTransport {
    NativeService,
    AppServerStdio,
    AcpStdio,
    StructuredCli,
    StdioRpc,
    HttpSse,
    NativeGateway,
}

impl HarnessTransport {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NativeService => "native_service",
            Self::AppServerStdio => "app_server_stdio",
            Self::AcpStdio => "acp_stdio",
            Self::StructuredCli => "structured_cli",
            Self::StdioRpc => "stdio_rpc",
            Self::HttpSse => "http_sse",
            Self::NativeGateway => "native_gateway",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinHarnessSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub transport: HarnessTransport,
    pub session_prefix: &'static str,
    /// Executable names checked in order (first match on PATH/user bin dirs wins).
    pub executables: &'static [&'static str],
    /// Arguments passed to launch ACP over stdio (empty if not an ACP CLI).
    pub acp_args: &'static [&'static str],
    /// Relative directory under `$HOME` where local CLI state/sessions may live.
    pub data_dir_rel: &'static str,
    /// Whether `setup_native_session` & `dispatch_to_node_session` can launch sessions.
    pub dispatchable: bool,
    /// Whether `probe_native_harness` supports read-only CLI probing.
    pub probeable: bool,
    /// Whether interactive approval cards / `native_permission` are supported.
    pub supports_approval: bool,
    /// Official human-facing login hint (never extracts credentials).
    pub login_hint: &'static str,
}

pub const BUILTIN_HARNESSES: &[BuiltinHarnessSpec] = &[
    BuiltinHarnessSpec {
        id: "dsh",
        label: "DSH",
        transport: HarnessTransport::NativeService,
        session_prefix: "session-",
        executables: &["dsh"],
        acp_args: &[],
        data_dir_rel: ".dsh",
        dispatchable: true,
        probeable: false,
        supports_approval: true,
        login_hint: "Ensure DSH Host is running and provide its HTTPS endpoint or controlled startup log path for token-to-cookie exchange.",
    },
    BuiltinHarnessSpec {
        id: "codex",
        label: "Codex",
        transport: HarnessTransport::AppServerStdio,
        session_prefix: "codex",
        executables: &["codex"],
        acp_args: &[],
        data_dir_rel: ".codex",
        dispatchable: true,
        probeable: true,
        supports_approval: true,
        login_hint: "Run `codex login` on the node via official CLI; Bob reuses the native local session state without extracting tokens.",
    },
    BuiltinHarnessSpec {
        id: "agy",
        label: "Antigravity",
        transport: HarnessTransport::AcpStdio,
        session_prefix: "agy",
        executables: &["agy"],
        acp_args: &["acp", "--stdio"],
        data_dir_rel: ".gemini/antigravity-cli",
        dispatchable: true,
        probeable: true,
        supports_approval: true,
        login_hint: "Run the official `agy` CLI under the same OS user on the node to complete interactive login; Bob never copies AGY credentials across machines.",
    },
    BuiltinHarnessSpec {
        id: "claude",
        label: "Claude Code",
        transport: HarnessTransport::StructuredCli,
        session_prefix: "claude-jsonl",
        executables: &["claude"],
        acp_args: &[],
        data_dir_rel: ".claude",
        dispatchable: true,
        probeable: true,
        supports_approval: true,
        login_hint: "Run `claude` on the node under the target OS user to complete official login or configure ANTHROPIC_API_KEY in the host environment.",
    },
    BuiltinHarnessSpec {
        id: "pi",
        label: "Pi",
        transport: HarnessTransport::StdioRpc,
        session_prefix: "pi",
        executables: &["pi"],
        acp_args: &[],
        data_dir_rel: ".pi/agent/sessions",
        dispatchable: true,
        probeable: true,
        supports_approval: false,
        login_hint: "Run `pi` on the node to complete provider login via Pi's native authentication flow.",
    },
    BuiltinHarnessSpec {
        id: "copilot",
        label: "Copilot",
        transport: HarnessTransport::AcpStdio,
        session_prefix: "copilot",
        executables: &["copilot"],
        acp_args: &["--acp", "--stdio"],
        data_dir_rel: ".copilot",
        dispatchable: true,
        probeable: true,
        supports_approval: true,
        login_hint: "Run `copilot login` on the node under the target OS user to authenticate with GitHub Copilot; Bob communicates via `copilot --acp --stdio`.",
    },
    BuiltinHarnessSpec {
        id: "kimi",
        label: "Kimi Code",
        transport: HarnessTransport::AcpStdio,
        session_prefix: "kimi",
        executables: &["kimi"],
        acp_args: &["acp"],
        data_dir_rel: ".kimi",
        dispatchable: true,
        probeable: true,
        supports_approval: true,
        login_hint: "Run `kimi acp --login` (or `kimi login`) on the node under the target OS user; Bob communicates via `kimi acp`.",
    },
    BuiltinHarnessSpec {
        id: "qoder",
        label: "Qoder",
        transport: HarnessTransport::AcpStdio,
        session_prefix: "qoder",
        executables: &["qodercli", "qoder"],
        acp_args: &["--acp"],
        data_dir_rel: ".qoder",
        dispatchable: true,
        probeable: true,
        supports_approval: true,
        login_hint: "Run `qodercli login` on the node under the target OS user; Bob communicates via `qodercli --acp`.",
    },
    BuiltinHarnessSpec {
        id: "zcode",
        label: "ZCode",
        transport: HarnessTransport::AcpStdio,
        session_prefix: "zcode",
        executables: &["zcode", "codebuddy"],
        acp_args: &["--acp"],
        data_dir_rel: ".zcode",
        dispatchable: true,
        probeable: true,
        supports_approval: true,
        login_hint: "Run `zcode` or `codebuddy` on the node under the target OS user to sign in; Bob communicates via `--acp`.",
    },
    BuiltinHarnessSpec {
        id: "workbuddy",
        label: "WorkBuddy",
        transport: HarnessTransport::AcpStdio,
        session_prefix: "workbuddy",
        executables: &["workbuddy"],
        acp_args: &["--acp"],
        data_dir_rel: ".workbuddy",
        dispatchable: true,
        probeable: true,
        supports_approval: true,
        login_hint: "Run `workbuddy` on the node under the target OS user to sign in; Bob communicates via `workbuddy --acp`.",
    },
    BuiltinHarnessSpec {
        id: "goose",
        label: "Goose",
        transport: HarnessTransport::AcpStdio,
        session_prefix: "goose",
        executables: &["goose"],
        acp_args: &["acp"],
        data_dir_rel: ".config/goose",
        dispatchable: true,
        probeable: true,
        supports_approval: true,
        login_hint: "Run `goose configure` on the node under the target OS user; Bob communicates via `goose acp`.",
    },
    BuiltinHarnessSpec {
        id: "doubao",
        label: "Doubao CLI",
        transport: HarnessTransport::StructuredCli,
        session_prefix: "doubao",
        executables: &["arkcli", "doubao"],
        acp_args: &[],
        data_dir_rel: ".ark",
        dispatchable: false,
        probeable: true,
        supports_approval: false,
        login_hint: "Install `@volcengine/ark-cli` (`arkcli`) and run `arkcli auth login volc-sso` (or `--no-browser`) on the node to authenticate with Volcengine Ark / Doubao.",
    },
    BuiltinHarnessSpec {
        id: "hermes",
        label: "Hermes Agent",
        transport: HarnessTransport::HttpSse,
        session_prefix: "hermes",
        executables: &["hermes"],
        acp_args: &[],
        data_dir_rel: ".hermes",
        dispatchable: false,
        probeable: false,
        supports_approval: false,
        login_hint: "Ensure Hermes API server is running and configure the profile-scoped API_SERVER_KEY on the node.",
    },
    BuiltinHarnessSpec {
        id: "openclaw",
        label: "OpenClaw",
        transport: HarnessTransport::NativeGateway,
        session_prefix: "openclaw",
        executables: &["openclaw"],
        acp_args: &[],
        data_dir_rel: ".openclaw",
        dispatchable: false,
        probeable: false,
        supports_approval: false,
        login_hint: "Run OpenClaw device pairing approval on the node gateway and grant required operator scopes.",
    },
];

pub const ALL_TOOL_IDS: [&str; 14] = [
    "dsh",
    "codex",
    "agy",
    "claude",
    "pi",
    "copilot",
    "kimi",
    "qoder",
    "zcode",
    "workbuddy",
    "goose",
    "doubao",
    "hermes",
    "openclaw",
];

pub const INVENTORY_TOOL_IDS: [&str; 12] = [
    "dsh",
    "codex",
    "agy",
    "claude",
    "pi",
    "copilot",
    "kimi",
    "qoder",
    "zcode",
    "workbuddy",
    "goose",
    "doubao",
];

pub const DISPATCHABLE_TOOL_IDS: [&str; 11] = [
    "dsh",
    "codex",
    "agy",
    "claude",
    "pi",
    "copilot",
    "kimi",
    "qoder",
    "zcode",
    "workbuddy",
    "goose",
];

pub const GENERIC_ACP_TOOL_IDS: [&str; 5] = [
    "kimi",
    "qoder",
    "zcode",
    "workbuddy",
    "goose",
];

pub fn find_builtin(id: &str) -> Option<&'static BuiltinHarnessSpec> {
    BUILTIN_HARNESSES.iter().find(|spec| spec.id == id)
}

pub fn is_known_tool(id: &str) -> bool {
    find_builtin(id).is_some()
}

pub fn is_dispatchable_tool(id: &str) -> bool {
    find_builtin(id).is_some_and(|s| s.dispatchable)
}

pub fn is_probeable_native_tool(id: &str) -> bool {
    find_builtin(id).is_some_and(|s| s.probeable)
}

pub fn is_acp_tool(id: &str) -> bool {
    find_builtin(id).is_some_and(|s| s.transport == HarnessTransport::AcpStdio)
}

pub fn is_generic_acp_tool(id: &str) -> bool {
    GENERIC_ACP_TOOL_IDS.contains(&id)
}

pub fn supports_native_setup(id: &str) -> bool {
    id != "dsh" && is_dispatchable_tool(id)
}

pub fn executable_aliases(id: &str) -> &'static [&'static str] {
    find_builtin(id).map(|s| s.executables).unwrap_or(&[])
}

pub fn session_tool_id(session_id: &str) -> &'static str {
    if session_id.starts_with("claude") {
        return "claude";
    }
    if let Some((prefix, _)) = session_id.split_once(':') {
        if let Some(spec) = BUILTIN_HARNESSES.iter().find(|s| s.session_prefix == prefix || s.id == prefix) {
            return spec.id;
        }
    }
    "dsh"
}

pub fn owns_native_session_prefix(session_id: &str) -> bool {
    if session_id.starts_with("agy:")
        || session_id.starts_with("claude-jsonl:")
        || session_id.starts_with("pi:")
        || session_id.starts_with("codex:")
        || session_id.starts_with("copilot:")
        || session_id.starts_with("cp:")
    {
        return true;
    }
    session_id
        .split_once(':')
        .is_some_and(|(prefix, _)| GENERIC_ACP_TOOL_IDS.contains(&prefix))
}

pub fn session_supports_approval(session_id: &str) -> bool {
    let tool = session_tool_id(session_id);
    if tool == "dsh" && !session_id.starts_with("session-") {
        return false;
    }
    find_builtin(tool).is_some_and(|s| s.supports_approval)
}

pub fn session_supports_cancel(session_id: &str) -> bool {
    if session_id.starts_with("codex:")
        || session_id.starts_with("pi:")
        || session_id.starts_with("agy:")
        || session_id.starts_with("copilot:")
        || session_id.starts_with("claude-jsonl:")
    {
        return true;
    }
    session_id
        .split_once(':')
        .is_some_and(|(prefix, _)| GENERIC_ACP_TOOL_IDS.contains(&prefix))
}

/// Optional locally-trusted custom ACP CLI entry persisted in `<data_dir>/acp_registry.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CustomAcpEntry {
    pub id: String,
    pub label: String,
    pub executable: String,
    #[serde(default)]
    pub acp_args: Vec<String>,
    #[serde(default)]
    pub login_hint: Option<String>,
}

fn is_safe_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 32
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        && s.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
}

fn is_safe_executable_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && !s.contains('/')
        && !s.contains('\\')
        && !s.contains("..")
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

fn is_safe_arg(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && !s.chars().any(char::is_control)
        && !s.contains(';')
        && !s.contains('|')
        && !s.contains('&')
        && !s.contains('`')
        && !s.contains('$')
}

pub fn load_custom_acp_registry(data_dir: &Path) -> Vec<CustomAcpEntry> {
    let path = data_dir.join("acp_registry.json");
    let Ok(bytes) = std::fs::read(&path) else {
        return Vec::new();
    };
    if bytes.len() > 65_536 {
        return Vec::new();
    }
    let Ok(entries) = serde_json::from_slice::<Vec<CustomAcpEntry>>(&bytes) else {
        return Vec::new();
    };
    entries
        .into_iter()
        .filter(|e| {
            is_safe_identifier(&e.id)
                && !is_known_tool(&e.id)
                && is_safe_executable_name(&e.executable)
                && e.acp_args.len() <= 8
                && e.acp_args.iter().all(|a| is_safe_arg(a))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_covers_acp_and_doubao_cli() {
        assert_eq!(BUILTIN_HARNESSES.len(), ALL_TOOL_IDS.len());
        for id in ["codex", "agy", "claude", "pi", "copilot", "kimi", "qoder", "zcode", "workbuddy", "goose"] {
            assert!(is_dispatchable_tool(id), "{id} should be dispatchable");
            assert!(supports_native_setup(id), "{id} should support native setup");
        }
        assert!(is_acp_tool("kimi"));
        assert!(is_acp_tool("qoder"));
        assert!(is_acp_tool("zcode"));
        assert!(is_acp_tool("workbuddy"));
        assert!(is_acp_tool("goose"));
        assert_eq!(executable_aliases("qoder"), &["qodercli", "qoder"]);
        assert_eq!(executable_aliases("zcode"), &["zcode", "codebuddy"]);
        assert_eq!(executable_aliases("doubao"), &["arkcli", "doubao"]);
        assert!(is_probeable_native_tool("doubao"));
        assert!(!is_dispatchable_tool("doubao"));
        assert_eq!(session_tool_id("kimi:abc12345"), "kimi");
        assert_eq!(session_tool_id("qoder:abc12345"), "qoder");
        assert!(owns_native_session_prefix("kimi:abc12345"));
        assert!(owns_native_session_prefix("workbuddy:abc12345"));
        assert!(session_supports_approval("kimi:abc12345"));
        assert!(session_supports_cancel("goose:abc12345"));
    }

    #[test]
    fn custom_acp_registry_rejects_unsafe_entries() {
        let dir = std::env::temp_dir().join(format!(
            "bob-acp-reg-{}-{}",
            std::process::id(),
            crate::now_millis()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let raw = serde_json::json!([
            {
                "id": "junie",
                "label": "Junie",
                "executable": "junie",
                "acpArgs": ["--acp"]
            },
            {
                "id": "bad/path",
                "label": "Bad",
                "executable": "sh",
                "acpArgs": ["-c", "id"]
            },
            {
                "id": "evil",
                "label": "Evil",
                "executable": "/bin/sh",
                "acpArgs": []
            },
            {
                "id": "kimi",
                "label": "Duplicate Builtin",
                "executable": "kimi",
                "acpArgs": ["acp"]
            }
        ]);
        std::fs::write(dir.join("acp_registry.json"), serde_json::to_vec(&raw).unwrap()).unwrap();
        let loaded = load_custom_acp_registry(&dir);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "junie");
        assert_eq!(loaded[0].executable, "junie");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
