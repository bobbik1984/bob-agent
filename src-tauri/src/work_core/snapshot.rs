use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::models::{validate_project_id, ProjectAggregate, WorkObject, WORK_SCHEMA_VERSION};

#[derive(Debug, Clone, Serialize)]
struct SnapshotFrontmatter<'a> {
    schema_version: u32,
    r#type: &'static str,
    project_id: &'a str,
    project_revision: u64,
    generated_at: String,
    generated_by: &'static str,
    read_only_snapshot: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotOutcome {
    pub relative_path: String,
    pub byte_size: usize,
}

fn render_items(title: &str, items: &[WorkObject], output: &mut String) {
    output.push_str(&format!("## {title}\n\n"));
    if items.is_empty() {
        output.push_str("暂无。\n\n");
        return;
    }
    for item in items {
        output.push_str(&format!(
            "- **{}** (`{}` · `{}`)\n",
            item.title, item.status, item.id
        ));
        if let Some(description) = item
            .description
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            output.push_str(&format!("  - {}\n", description.trim()));
        }
        if item.kind == super::models::WorkObjectKind::Goal {
            if let Some(outcome) = item.data.get("outcome").and_then(|value| value.as_str()) {
                output.push_str(&format!("  - 期望结果：{}\n", outcome.trim()));
            }
        }
        if item.kind == super::models::WorkObjectKind::Decision {
            if let Some(decision) = item.data.get("decision").and_then(|value| value.as_str()) {
                output.push_str(&format!("  - 决定：{}\n", decision.trim()));
            }
            if let Some(reason) = item.data.get("reason").and_then(|value| value.as_str()) {
                output.push_str(&format!("  - 理由：{}\n", reason.trim()));
            }
        }
    }
    output.push('\n');
}

pub fn render_project_snapshot(aggregate: &ProjectAggregate) -> Result<String, String> {
    validate_project_id(&aggregate.project.id)?;
    let frontmatter = SnapshotFrontmatter {
        schema_version: WORK_SCHEMA_VERSION,
        r#type: "project_state_snapshot",
        project_id: &aggregate.project.id,
        project_revision: aggregate.project.revision,
        generated_at: chrono::Local::now().to_rfc3339(),
        generated_by: "bob-work-core",
        read_only_snapshot: true,
    };
    let yaml = serde_yaml::to_string(&frontmatter).map_err(|error| error.to_string())?;
    let mut output = format!(
        "---\n{yaml}---\n\n# {}\n\n> 本文件由 Bob Work Core 生成，用于跨 Agent 阅读和迁移。请勿直接编辑以覆盖运行状态。\n\n",
        aggregate.project.title
    );
    if !aggregate.project.mission.trim().is_empty() {
        output.push_str(&format!("**项目使命：** {}\n\n", aggregate.project.mission));
    }
    output.push_str(&format!(
        "- 状态：`{}`\n- 当前阶段：{}\n- 修订：{}\n\n",
        aggregate.project.status,
        aggregate
            .project
            .current_phase
            .as_deref()
            .unwrap_or("未设置"),
        aggregate.project.revision
    ));
    if let Some(summary) = aggregate
        .project
        .summary
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        output.push_str(&format!("## 当前摘要\n\n{}\n\n", summary.trim()));
    }
    render_items("目标", &aggregate.goals, &mut output);
    render_items("里程碑", &aggregate.milestones, &mut output);
    render_items("任务", &aggregate.tasks, &mut output);
    render_items("决定", &aggregate.decisions, &mut output);
    render_items("风险", &aggregate.risks, &mut output);
    render_items("近期变化", &aggregate.changes, &mut output);
    render_items("承诺", &aggregate.commitments, &mut output);
    render_items("产物", &aggregate.artifacts, &mut output);
    render_items("证据", &aggregate.evidence, &mut output);
    Ok(output)
}

fn write_atomically(path: &Path, content: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "项目快照路径缺少父目录".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("无法创建项目快照目录: {error}"))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "项目快照文件名无效".to_string())?;
    let temp = parent.join(format!(".{file_name}.{}.part", ulid::Ulid::new()));
    let backup = parent.join(format!(".{file_name}.{}.backup", ulid::Ulid::new()));
    let result = (|| -> Result<(), String> {
        let mut file = fs::File::create(&temp).map_err(|error| error.to_string())?;
        file.write_all(content.as_bytes())
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        if path.exists() {
            fs::rename(path, &backup).map_err(|error| error.to_string())?;
        }
        if let Err(error) = fs::rename(&temp, path) {
            if backup.exists() {
                let _ = fs::rename(&backup, path);
            }
            return Err(format!("无法提交项目快照: {error}"));
        }
        if backup.exists() {
            fs::remove_file(&backup)
                .map_err(|error| format!("项目快照已保存，但旧快照清理失败: {error}"))?;
        }
        Ok(())
    })();
    if temp.exists() {
        let _ = fs::remove_file(temp);
    }
    result
}

pub fn write_project_snapshot_at(
    aggregate: &ProjectAggregate,
    notes_root: &Path,
) -> Result<SnapshotOutcome, String> {
    validate_project_id(&aggregate.project.id)?;
    let relative = PathBuf::from("notes/projects")
        .join(&aggregate.project.id)
        .join("_PROJECT_STATE.md");
    let absolute = notes_root
        .join("projects")
        .join(&aggregate.project.id)
        .join("_PROJECT_STATE.md");
    let rendered = render_project_snapshot(aggregate)?;
    write_atomically(&absolute, &rendered)?;
    Ok(SnapshotOutcome {
        relative_path: relative.to_string_lossy().replace('\\', "/"),
        byte_size: rendered.len(),
    })
}

pub fn write_project_snapshot(aggregate: &ProjectAggregate) -> Result<SnapshotOutcome, String> {
    write_project_snapshot_at(aggregate, &crate::notebook::get_notes_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work_core::models::{WorkObjectKind, WorkProject};
    use serde_json::json;

    fn object(kind: WorkObjectKind, id: &str, title: &str, data: serde_json::Value) -> WorkObject {
        WorkObject {
            schema_version: WORK_SCHEMA_VERSION,
            id: id.into(),
            kind,
            project_id: "project_demo".into(),
            parent_id: None,
            title: title.into(),
            status: "active".into(),
            description: None,
            data,
            source_capture_id: None,
            revision: 1,
            created_at: 1,
            updated_at: 1,
            deleted_at: None,
        }
    }

    fn aggregate() -> ProjectAggregate {
        ProjectAggregate {
            project: WorkProject {
                schema_version: WORK_SCHEMA_VERSION,
                id: "project_demo".into(),
                title: "示例项目".into(),
                mission: "让项目状态可恢复".into(),
                status: "active".into(),
                current_phase: Some("方案".into()),
                summary: Some("正在确认核心决定。".into()),
                source_ref: None,
                metadata: json!({}),
                revision: 3,
                created_at: 1,
                updated_at: 2,
                deleted_at: None,
            },
            responsibilities: vec![],
            goals: vec![object(
                WorkObjectKind::Goal,
                "goal_demo",
                "完成方案",
                json!({ "outcome": "方案通过" }),
            )],
            milestones: vec![],
            tasks: vec![],
            decisions: vec![object(
                WorkObjectKind::Decision,
                "decision_demo",
                "采用渐进演进",
                json!({ "decision": "保留现有客户端", "reason": "降低迁移风险" }),
            )],
            artifacts: vec![],
            evidence: vec![],
            risks: vec![],
            changes: vec![],
            commitments: vec![],
            recent_events: vec![],
        }
    }

    #[test]
    fn snapshot_contains_identity_decision_reason_and_warning() {
        let rendered = render_project_snapshot(&aggregate()).unwrap();
        assert!(rendered.contains("project_id: project_demo"));
        assert!(rendered.contains("请勿直接编辑"));
        assert!(rendered.contains("采用渐进演进"));
        assert!(rendered.contains("降低迁移风险"));
    }

    #[test]
    fn snapshot_write_is_atomic_and_replaces_previous_copy() {
        let root = std::env::temp_dir().join(format!("bob-work-snapshot-{}", ulid::Ulid::new()));
        let first = write_project_snapshot_at(&aggregate(), &root).unwrap();
        let second = write_project_snapshot_at(&aggregate(), &root).unwrap();
        assert_eq!(first.relative_path, second.relative_path);
        let directory = root.join("projects/project_demo");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        let saved = fs::read_to_string(directory.join("_PROJECT_STATE.md")).unwrap();
        assert!(saved.contains("project_revision: 3"));
        fs::remove_dir_all(root).unwrap();
    }
}
