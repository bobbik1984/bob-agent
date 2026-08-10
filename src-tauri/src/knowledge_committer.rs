use md5::{Digest, Md5};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use crate::capture::CaptureEnvelope;
use crate::capture_router::{RouteDecision, RouteIntent};
use crate::knowledge_schema::{
    parse_markdown, suggested_relative_path, write_markdown_safely, KnowledgeFrontmatter,
    KnowledgeMarkdown, KnowledgeObjectType, KNOWLEDGE_SCHEMA_VERSION,
};

#[derive(Debug, Clone)]
pub struct CommitOutcome {
    pub object_id: String,
    pub relative_path: String,
    pub duplicate: bool,
}

pub const NEEDS_PROJECT_CLARIFICATION: &str = "NEEDS_PROJECT_CLARIFICATION:";

pub fn is_project_clarification_error(error: &str) -> bool {
    error.starts_with(NEEDS_PROJECT_CLARIFICATION)
}

fn normalized_project_name(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|ch| !ch.is_whitespace() && !matches!(ch, '-' | '_' | '·'))
        .collect()
}

fn collect_markdown_files(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect_markdown_files(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            files.push(path);
        }
    }
}

fn resolve_project_hint(wiki_root: &Path, hint: &str) -> Result<String, String> {
    let wanted = normalized_project_name(hint);
    if wanted.is_empty() {
        return Err(format!(
            "{NEEDS_PROJECT_CLARIFICATION}项目名称为空，请选择一个现有项目"
        ));
    }
    let mut files = Vec::new();
    for directory in ["entities", "projects"] {
        collect_markdown_files(&wiki_root.join(directory), &mut files);
    }
    let mut matches = Vec::new();
    for path in files {
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(document) = parse_markdown(&raw) else {
            continue;
        };
        if document.frontmatter.object_type.as_deref() != Some("project") {
            continue;
        }
        let Some(id) = document.frontmatter.id.as_deref() else {
            continue;
        };
        if id.starts_with("project_")
            && (normalized_project_name(&document.frontmatter.title) == wanted
                || normalized_project_name(id) == wanted)
        {
            matches.push(id.to_string());
        }
    }
    matches.sort();
    matches.dedup();
    match matches.as_slice() {
        [id] => Ok(id.clone()),
        [] => Err(format!(
            "{NEEDS_PROJECT_CLARIFICATION}未找到名称为“{}”的项目，请选择现有项目",
            hint.trim()
        )),
        _ => Err(format!(
            "{NEEDS_PROJECT_CLARIFICATION}名称“{}”匹配到多个项目，请选择具体项目",
            hint.trim()
        )),
    }
}

fn digest_hex(value: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn canonical_source_identity(capture: &CaptureEnvelope) -> String {
    if let Some(url) = capture.source_url.as_deref() {
        let trimmed = url
            .trim()
            .split('#')
            .next()
            .unwrap_or(url)
            .trim_end_matches('/');
        return trimmed.to_string();
    }
    if let Some(path) = capture.file_path.as_deref() {
        return format!("file:{}", path.trim().replace('\\', "/").to_lowercase());
    }
    format!("content:{}", capture.content_hash)
}

fn stable_object_id(object_type: KnowledgeObjectType, capture: &CaptureEnvelope) -> String {
    let identity = if object_type == KnowledgeObjectType::Source {
        canonical_source_identity(capture)
    } else {
        capture.content_hash.clone()
    };
    format!(
        "{}_{}",
        object_type.id_prefix(),
        &digest_hex(&identity)[..20]
    )
}

fn safe_slug(title: &str) -> String {
    let slug = title
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    let compact = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if compact.is_empty() {
        "untitled".into()
    } else {
        compact.chars().take(60).collect()
    }
}

fn resolve_relative(
    relative: &Path,
    notes_root: &Path,
    wiki_root: &Path,
) -> Result<PathBuf, String> {
    let normalized = relative.to_string_lossy().replace('\\', "/");
    if let Some(rest) = normalized.strip_prefix("notes/") {
        return Ok(notes_root.join(rest));
    }
    if let Some(rest) = normalized.strip_prefix("wiki/") {
        return Ok(wiki_root.join(rest));
    }
    Err(format!(
        "知识对象路径不在允许的 notes/wiki 范围内: {normalized}"
    ))
}

fn verify_existing(path: &Path, expected_id: &str) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }
    let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let parsed = parse_markdown(&raw)?;
    match parsed.frontmatter.id.as_deref() {
        Some(id) if id == expected_id => Ok(true),
        Some(id) => Err(format!("目标文件已存在但对象 ID 冲突: {id}")),
        None => Err("目标文件已存在但缺少稳定对象 ID".into()),
    }
}

fn base_frontmatter(
    capture: &CaptureEnvelope,
    route: &RouteDecision,
    object_type: KnowledgeObjectType,
    object_id: &str,
) -> KnowledgeFrontmatter {
    let now = chrono::Local::now().to_rfc3339();
    let mut extra = std::collections::BTreeMap::new();
    extra.insert("capture_refs".into(), json!([capture.capture_id]));
    extra.insert("source_device".into(), json!(capture.source_device));
    if let Some(url) = capture.source_url.as_deref() {
        extra.insert("source_url".into(), json!(url));
    }
    if let Some(path) = capture.file_path.as_deref() {
        extra.insert("source_path".into(), json!(path));
    }
    KnowledgeFrontmatter {
        id: Some(object_id.to_string()),
        schema_version: Some(KNOWLEDGE_SCHEMA_VERSION),
        object_type: Some(object_type.as_str().into()),
        title: route.title.trim().to_string(),
        tags: route.tags.clone(),
        domains: route.domains.clone(),
        topics: route.topics.clone(),
        project_id: if object_type == KnowledgeObjectType::Note {
            route.project_id.clone()
        } else {
            None
        },
        status: Some(
            if object_type == KnowledgeObjectType::Source
                && capture.source_url.is_some()
                && capture.file_path.is_none()
            {
                "pending_extraction".into()
            } else {
                "active".into()
            },
        ),
        confidence: Some(route.confidence.clamp(0.0, 1.0) as f64),
        created_by: Some("bob".into()),
        created_at: Some(now.clone()),
        updated_at: Some(now),
        extra,
        ..KnowledgeFrontmatter::default()
    }
}

fn source_body(capture: &CaptureEnvelope) -> String {
    let mut body = String::new();
    if let Some(url) = capture.source_url.as_deref() {
        body.push_str(&format!("> 来源：[{url}]({url})\n\n"));
    }
    if let Some(path) = capture.file_path.as_deref() {
        body.push_str(&format!("> 原始文件：`{path}`\n\n"));
    }
    if let Some(content) = capture.content.as_deref() {
        body.push_str(content.trim());
        body.push('\n');
    }
    body
}

pub fn commit_knowledge_capture(
    capture: &CaptureEnvelope,
    route: &RouteDecision,
) -> Result<CommitOutcome, String> {
    commit_knowledge_capture_at(
        capture,
        route,
        &crate::notebook::get_notes_dir(),
        &crate::get_wiki_dir(),
    )
}

fn commit_knowledge_capture_at(
    capture: &CaptureEnvelope,
    route: &RouteDecision,
    notes_root: &Path,
    wiki_root: &Path,
) -> Result<CommitOutcome, String> {
    let mut route = route.clone();
    if route.intent == RouteIntent::Note && route.project_id.is_none() {
        if let Some(hint) = route.project_hint.as_deref() {
            route.project_id = Some(resolve_project_hint(wiki_root, hint)?);
        }
    }
    if route.title.trim().is_empty() {
        return Err("知识对象缺少标题".into());
    }
    if let Some(project_id) = route.project_id.as_deref() {
        if route.intent != RouteIntent::Note || !project_id.starts_with("project_") {
            return Err("项目归属只允许使用 project_ 开头的 Note 项目 ID".into());
        }
    }

    if route.intent == RouteIntent::QuickNote {
        let content = capture
            .content
            .clone()
            .ok_or_else(|| "速记缺少原始内容".to_string())?;
        let result =
            crate::notebook::notebook_append_daily_capture(content, capture.capture_id.clone())?;
        let path = result
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| "速记提交后没有文件回执".to_string())?;
        return Ok(CommitOutcome {
            object_id: format!("capture_{}", capture.capture_id),
            relative_path: path.to_string(),
            duplicate: result
                .get("duplicate")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        });
    }

    let object_type = match route.intent {
        RouteIntent::Note => KnowledgeObjectType::Note,
        RouteIntent::Source => KnowledgeObjectType::Source,
        _ => return Err("该分类尚不属于知识提交管线".into()),
    };
    let object_id = stable_object_id(object_type, capture);
    let slug = safe_slug(&route.title);
    let relative = if object_type == KnowledgeObjectType::Note {
        if let Some(project_id) = route.project_id.as_deref() {
            PathBuf::from("notes/projects")
                .join(project_id)
                .join(format!("{object_id}--{slug}.md"))
        } else {
            suggested_relative_path(object_type, &object_id, &slug)
        }
    } else {
        suggested_relative_path(object_type, &object_id, &slug)
    };
    let absolute = resolve_relative(&relative, notes_root, wiki_root)?;
    if !absolute.exists() {
        if let Some(parent) = absolute.parent() {
            if let Ok(entries) = std::fs::read_dir(parent) {
                let prefix = format!("{object_id}--");
                for entry in entries.filter_map(Result::ok) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with(&prefix) && name.ends_with(".md") {
                        let existing_path = entry.path();
                        if verify_existing(&existing_path, &object_id)? {
                            let existing_relative = relative
                                .parent()
                                .unwrap_or_else(|| Path::new(""))
                                .join(name)
                                .to_string_lossy()
                                .replace('\\', "/");
                            return Ok(CommitOutcome {
                                object_id,
                                relative_path: existing_relative,
                                duplicate: true,
                            });
                        }
                    }
                }
            }
        }
    }
    if verify_existing(&absolute, &object_id)? {
        return Ok(CommitOutcome {
            object_id,
            relative_path: relative.to_string_lossy().replace('\\', "/"),
            duplicate: true,
        });
    }

    let document = KnowledgeMarkdown {
        frontmatter: base_frontmatter(capture, &route, object_type, &object_id),
        body: if object_type == KnowledgeObjectType::Source {
            source_body(capture)
        } else {
            capture.content.clone().unwrap_or_default()
        },
        had_frontmatter: true,
    };
    write_markdown_safely(&absolute, &document)?;
    if !verify_existing(&absolute, &object_id)? {
        return Err("Markdown 写入后未获得对象回执".into());
    }
    Ok(CommitOutcome {
        object_id,
        relative_path: relative.to_string_lossy().replace('\\', "/"),
        duplicate: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::{build_envelope_for_test, CaptureInput};

    fn capture(content: &str, url: Option<&str>) -> CaptureEnvelope {
        build_envelope_for_test(CaptureInput {
            entry_point: "test".into(),
            content: Some(content.into()),
            source_url: url.map(str::to_string),
            file_path: None,
            explicit_intent: None,
            language: Some("zh-CN".into()),
            privacy_scope: None,
            sync_scope: None,
            source_device: Some("test".into()),
            idempotency_key: None,
        })
        .unwrap()
    }

    fn route(intent: RouteIntent, title: &str) -> RouteDecision {
        RouteDecision {
            intent,
            title: title.into(),
            date: None,
            start_time: None,
            end_time: None,
            confidence: 0.9,
            needs_clarification: false,
            reason_codes: vec![],
            project_id: None,
            project_hint: None,
            tags: vec![],
            domains: vec![],
            topics: vec![],
        }
    }

    #[test]
    fn same_url_has_same_source_identity_despite_different_capture_text() {
        let first = capture("收藏文章", Some("https://example.com/a#part"));
        let second = capture("以后研究", Some("https://example.com/a"));
        assert_eq!(
            stable_object_id(KnowledgeObjectType::Source, &first),
            stable_object_id(KnowledgeObjectType::Source, &second)
        );
    }

    #[test]
    fn project_ownership_is_rejected_for_sources() {
        let capture = capture("来源", Some("https://example.com"));
        let mut route = route(RouteIntent::Source, "来源");
        route.project_id = Some("project_123".into());
        assert!(commit_knowledge_capture(&capture, &route).is_err());
    }

    #[test]
    fn note_write_is_idempotent_and_preserves_stable_id() {
        let root = std::env::temp_dir().join(format!("bob-committer-{}", ulid::Ulid::new()));
        let notes = root.join("notes");
        let wiki = root.join("wiki");
        let capture = capture("完整的个人思考", None);
        let route = route(RouteIntent::Note, "个人思考");
        let first = commit_knowledge_capture_at(&capture, &route, &notes, &wiki).unwrap();
        let second = commit_knowledge_capture_at(&capture, &route, &notes, &wiki).unwrap();
        assert_eq!(first.object_id, second.object_id);
        assert_eq!(first.relative_path, second.relative_path);
        assert!(!first.duplicate);
        assert!(second.duplicate);
        let saved =
            std::fs::read_to_string(notes.join(first.relative_path.trim_start_matches("notes/")))
                .unwrap();
        assert!(saved.contains(&first.object_id));
        assert!(saved.contains(&capture.capture_id));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn same_url_reuses_existing_source_even_when_title_changes() {
        let root = std::env::temp_dir().join(format!("bob-source-{}", ulid::Ulid::new()));
        let notes = root.join("notes");
        let wiki = root.join("wiki");
        let first_capture = capture("第一次收藏", Some("https://example.com/article#section"));
        let second_capture = capture("再次收藏", Some("https://example.com/article"));
        let first = commit_knowledge_capture_at(
            &first_capture,
            &route(RouteIntent::Source, "第一版标题"),
            &notes,
            &wiki,
        )
        .unwrap();
        let second = commit_knowledge_capture_at(
            &second_capture,
            &route(RouteIntent::Source, "另一个标题"),
            &notes,
            &wiki,
        )
        .unwrap();
        assert_eq!(first.object_id, second.object_id);
        assert_eq!(first.relative_path, second.relative_path);
        assert!(second.duplicate);
        let count = std::fs::read_dir(wiki.join("sources")).unwrap().count();
        assert_eq!(count, 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn write_project(wiki: &Path, id: &str, title: &str, suffix: &str) {
        let path = wiki.join("entities").join(format!("{suffix}.md"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            format!(
                "---\nid: {id}\nschema_version: 1\ntype: project\ntitle: {title}\n---\n\n项目说明"
            ),
        )
        .unwrap();
    }

    #[test]
    fn project_hint_is_resolved_only_when_exactly_one_project_matches() {
        let root = std::env::temp_dir().join(format!("bob-project-{}", ulid::Ulid::new()));
        let notes = root.join("notes");
        let wiki = root.join("wiki");
        write_project(&wiki, "project_bob", "Bob Agent", "bob");
        let capture = capture("这是 Bob Agent 项目的完整思考", None);
        let mut route = route(RouteIntent::Note, "项目思考");
        route.project_hint = Some("bob-agent".into());

        let outcome = commit_knowledge_capture_at(&capture, &route, &notes, &wiki).unwrap();
        assert!(outcome
            .relative_path
            .starts_with("notes/projects/project_bob/"));
        let raw =
            std::fs::read_to_string(notes.join(outcome.relative_path.trim_start_matches("notes/")))
                .unwrap();
        assert!(raw.contains("project_id: project_bob"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unknown_project_hint_requires_clarification_without_writing_note() {
        let root = std::env::temp_dir().join(format!("bob-project-{}", ulid::Ulid::new()));
        let notes = root.join("notes");
        let wiki = root.join("wiki");
        let capture = capture("这是未知项目的笔记", None);
        let mut route = route(RouteIntent::Note, "项目笔记");
        route.project_hint = Some("不存在的项目".into());

        let error = commit_knowledge_capture_at(&capture, &route, &notes, &wiki).unwrap_err();
        assert!(is_project_clarification_error(&error));
        assert!(!notes.exists());
        std::fs::remove_dir_all(root).ok();
    }
}
