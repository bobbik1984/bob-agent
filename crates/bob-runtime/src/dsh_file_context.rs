//! Relative links belong to a reply's execution context, not the CLI launch folder.
//! Read only structured native history. Never execute shell text or search other repos.
use crate::dsh_session::History;
use serde_json::Value;
use std::collections::BTreeSet;

fn relative_base(root: &str, cwd: &str) -> Option<String> {
    let root = root.trim_end_matches('/');
    let cwd = cwd.trim_end_matches('/');
    if cwd.contains(['\\', '$', '`', '\0', '\n', '\r']) { return None; }
    if cwd.split('/').any(|p| p == ".." || p == ".") { return None; }
    if cwd == root { return Some(String::new()); }
    cwd.strip_prefix(&format!("{root}/")).filter(|s| !s.is_empty()).map(str::to_owned)
}

// Only a literal leading `cd DIR && ...` is recognized. This is a bounded
// compatibility adapter: the initial directory is a candidate, never an inferred
// final shell directory. The command remainder is not interpreted or executed.
fn command_cwd(command: &str) -> Option<&str> {
    let rest = command.trim_start().strip_prefix("cd ")?.trim_start();
    let (dir, _) = rest.split_once("&&")?;
    let dir = dir.trim();
    let dir = if dir.starts_with('\'') && dir.ends_with('\'') || dir.starts_with('"') && dir.ends_with('"') {
        &dir[1..dir.len()-1]
    } else {
        if dir.chars().any(char::is_whitespace) { return None; }
        dir
    };
    if dir.is_empty() || dir.contains([';', '|', '&', '>', '<', '$', '`', '\\', '\n', '\r', '\'', '"']) { return None; }
    Some(dir)
}

pub(crate) fn message_contexts(history: &History, snapshot: &Value) -> Vec<(String, String, i64)> {
    let Some(root) = history.workspace.as_deref() else { return Vec::new(); };
    let records = snapshot["records"].as_array();
    history.messages.iter().filter(|m| m.role == "assistant").flat_map(|message| {
        let start = history.messages.iter().filter(|m| m.role == "user" && m.sequence < message.sequence)
            .map(|m| m.sequence).max().unwrap_or(-1);
        let mut bases = BTreeSet::new();
        for row in records.into_iter().flatten() {
            let event = &row["event"];
            let Some(seq) = event["seq"].as_i64() else { continue; };
            if seq <= start || seq >= message.sequence || event["type"] != "tool/call" { continue; }
            let data = &event["data"];
            let parsed;
            let args = if let Some(raw) = data["arguments"].as_str() {
                parsed = serde_json::from_str::<Value>(raw).unwrap_or(Value::Null); &parsed
            } else { &data["arguments"] };
            let structured = ["cwd", "workdir", "workingDirectory"].iter().find_map(|k| args[*k].as_str());
            let command = if data["name"] == "bash" { args["command"].as_str() } else { None };
            if command.is_some_and(|c| c.trim_start().starts_with("cd ") && command_cwd(c).is_none()) {
                bases.insert("!ambiguous".into()); continue;
            }
            let cwd = command.and_then(command_cwd).or(structured);
            if let Some(cwd) = cwd {
                let absolute = if cwd.starts_with('/') { cwd.to_owned() } else { format!("{}/{cwd}", structured.unwrap_or(root).trim_end_matches('/')) };
                bases.insert(relative_base(root, &absolute).unwrap_or_else(|| "!ambiguous".into()));
            } else if command.is_some_and(|c| c.trim_start().starts_with("cd ")) {
                bases.insert("!ambiguous".into());
            }
        }
        if bases.is_empty() { bases.insert(String::new()); }
        bases.into_iter().map(|base|(message.text.clone(), base, message.sequence)).collect::<Vec<_>>()
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsh_session::{Message, SessionAddress};
    use serde_json::json;
    fn history() -> History { History { address: SessionAddress {connection_id:"local".into(),session_id:"s".into()}, workspace:Some("/work".into()), partial:false, omitted:0, request_results:vec![], messages:vec![Message {sequence:1,role:"user".into(),text:"update".into(),created_at:None},Message {sequence:5,role:"assistant".into(),text:"[Read](README.md)".into(),created_at:None}] } }
    fn call(seq:i64, command:&str)->Value {json!({"event":{"type":"tool/call","seq":seq,"data":{"name":"bash","arguments":json!({"command":command}).to_string()}}})}
    #[test] fn subproject_links_use_native_turn_context() {
        assert_eq!(message_contexts(&history(),&json!({"records":[call(2,"cd /work/project && cat README.md")]}))[0].1,"project");
    }
    #[test] fn launch_root_and_no_context_remain_compatible() {
        assert_eq!(message_contexts(&history(),&json!({"records":[]}))[0].1,"");
        assert_eq!(relative_base("/work","/work"),Some(String::new()));
    }
    #[test] fn conflicting_projects_do_not_choose_a_file() {
        let candidates=message_contexts(&history(),&json!({"records":[call(2,"cd /work/a && ls"),call(3,"cd /work/b && ls")]}));
        assert_eq!(candidates.iter().map(|c|c.1.as_str()).collect::<Vec<_>>(),vec!["a","b"]);
    }
    #[test] fn old_turn_or_future_tools_do_not_rebind_links() {
        assert_eq!(message_contexts(&history(),&json!({"records":[call(0,"cd /work/old && ls"),call(6,"cd /work/future && ls")]}))[0].1,"");
    }
    #[test] fn escape_and_dynamic_shell_are_not_context() {
        for command in ["cd /other && ls","cd /work/../other && ls","cd $HOME && ls"] {
            assert_eq!(message_contexts(&history(),&json!({"records":[call(2,command)]}))[0].1,"!ambiguous");
        }
        assert_eq!(relative_base("/work","/workspace/a"),None);
        assert_eq!(command_cwd("cd '/work/my project' && ls"),Some("/work/my project"));
    }
}
