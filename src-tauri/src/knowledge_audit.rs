use md5::{Digest, Md5};
use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::knowledge_schema::{parse_markdown, KnowledgeObjectType};

#[derive(Debug, Clone, Serialize)]
pub struct AuditedKnowledgeFile {
    pub path: String,
    pub root: String,
    pub relative_path: String,
    pub byte_length: u64,
    pub content_hash: String,
    pub title: String,
    pub declared_type: Option<String>,
    pub inferred_type: String,
    pub object_id: Option<String>,
    pub source_locator: Option<String>,
    pub suggested_target: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateGroup {
    pub key: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeAuditReport {
    pub schema_version: u32,
    pub scanned_at: String,
    pub read_only: bool,
    pub roots: Vec<String>,
    pub file_count: usize,
    pub files: Vec<AuditedKnowledgeFile>,
    pub exact_duplicates: Vec<DuplicateGroup>,
    pub source_duplicates: Vec<DuplicateGroup>,
    pub same_title_candidates: Vec<DuplicateGroup>,
    pub broken_wikilinks: BTreeMap<String, Vec<String>>,
    pub read_errors: Vec<String>,
    pub suggested_target_counts: BTreeMap<String, usize>,
}

fn slash_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn content_hash(bytes: &[u8]) -> String {
    let mut hasher = Md5::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn normalized_title(title: &str) -> String {
    title
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn scalar_extra(
    frontmatter: &crate::knowledge_schema::KnowledgeFrontmatter,
    key: &str,
) -> Option<String> {
    frontmatter.extra.get(key).and_then(|value| match value {
        serde_json::Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
        _ => None,
    })
}

fn infer_type(relative_path: &str, declared: Option<&str>) -> String {
    if let Some(value) = declared {
        if KnowledgeObjectType::parse(value).is_some() {
            return value.to_string();
        }
        if matches!(value, "user" | "feedback" | "reference") {
            return "legacy_memory".to_string();
        }
    }
    let path = relative_path.replace('\\', "/").to_lowercase();
    if path.contains("/sources/") {
        "source".to_string()
    } else if path.contains("/entities/") {
        "entity".to_string()
    } else if path.contains("/sessions/") {
        "session".to_string()
    } else if path.contains("/learned/") {
        "legacy_memory".to_string()
    } else if path.contains("/projects/") {
        "project_note".to_string()
    } else if path.contains("/topics/") {
        "note".to_string()
    } else if path.contains("/daily/") || path.contains("/quick/") {
        "quick_note".to_string()
    } else {
        "unclassified".to_string()
    }
}

fn suggested_target(inferred: &str, declared: Option<&str>) -> String {
    match (inferred, declared) {
        ("source", _) => "wiki/sources".to_string(),
        ("entity", _) => "wiki/entities".to_string(),
        ("session", _) => "wiki/sessions".to_string(),
        ("note", _) => "notes/topics".to_string(),
        ("project_note", _) => "notes/projects/<project-id>".to_string(),
        ("quick_note", _) => "notes/quick".to_string(),
        ("knowledge_point", _) => "wiki/knowledge".to_string(),
        ("memory", _) | ("legacy_memory", Some("user")) => "wiki/memories/user".to_string(),
        ("legacy_memory", Some("project")) => {
            "wiki/memories/project-or-knowledge-review".to_string()
        }
        ("legacy_memory", Some("feedback")) => "wiki/memories/corrections".to_string(),
        ("legacy_memory", Some("reference")) => "wiki/knowledge-or-procedural-review".to_string(),
        ("legacy_memory", _) => "wiki/memories/review".to_string(),
        _ => "review/unclassified".to_string(),
    }
}

fn group_duplicates(values: HashMap<String, Vec<String>>) -> Vec<DuplicateGroup> {
    let mut groups: Vec<DuplicateGroup> = values
        .into_iter()
        .filter(|(_, paths)| paths.len() > 1)
        .map(|(key, mut paths)| {
            paths.sort();
            DuplicateGroup { key, paths }
        })
        .collect();
    groups.sort_by(|left, right| left.key.cmp(&right.key));
    groups
}

pub fn audit_roots(roots: &[PathBuf]) -> KnowledgeAuditReport {
    let scanned_at = chrono::Local::now().to_rfc3339();
    let mut files = Vec::new();
    let mut read_errors = Vec::new();
    let mut hashes: HashMap<String, Vec<String>> = HashMap::new();
    let mut sources: HashMap<String, Vec<String>> = HashMap::new();
    let mut titles: HashMap<String, Vec<String>> = HashMap::new();
    let mut known_titles = BTreeSet::new();
    let mut wikilinks_by_path: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let wikilink_pattern = Regex::new(r"\[\[([^\[\]]+)\]\]").expect("valid wikilink regex");

    for root in roots {
        if !root.exists() {
            read_errors.push(format!("Root does not exist: {}", slash_path(root)));
            continue;
        }
        for entry in WalkDir::new(root).follow_links(false).into_iter() {
            let entry = match entry {
                Ok(value) => value,
                Err(error) => {
                    read_errors.push(error.to_string());
                    continue;
                }
            };
            let path = entry.path();
            if !entry.file_type().is_file()
                || path.extension().and_then(|ext| ext.to_str()) != Some("md")
            {
                continue;
            }

            let bytes = match fs::read(path) {
                Ok(value) => value,
                Err(error) => {
                    read_errors.push(format!("{}: {}", slash_path(path), error));
                    continue;
                }
            };
            let raw = match String::from_utf8(bytes.clone()) {
                Ok(value) => value,
                Err(error) => {
                    read_errors.push(format!("{}: invalid UTF-8 ({})", slash_path(path), error));
                    continue;
                }
            };
            let relative = path.strip_prefix(root).unwrap_or(path);
            let relative_path = slash_path(relative);
            let absolute_path = slash_path(path);
            let hash = content_hash(&bytes);
            let mut warnings = Vec::new();
            let parsed = match parse_markdown(&raw) {
                Ok(value) => value,
                Err(error) => {
                    warnings.push(error);
                    crate::knowledge_schema::KnowledgeMarkdown {
                        frontmatter: Default::default(),
                        body: raw.clone(),
                        had_frontmatter: false,
                    }
                }
            };
            if !parsed.had_frontmatter {
                warnings.push("missing_frontmatter".to_string());
            }
            if parsed.frontmatter.id.is_none() {
                warnings.push("missing_stable_id".to_string());
            }

            let fallback_title = path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("Untitled")
                .to_string();
            let title = if parsed.frontmatter.title.trim().is_empty() {
                raw.lines()
                    .find_map(|line| line.strip_prefix("# ").map(str::trim))
                    .filter(|value| !value.is_empty())
                    .unwrap_or(&fallback_title)
                    .to_string()
            } else {
                parsed.frontmatter.title.clone()
            };
            known_titles.insert(title.clone());
            known_titles.insert(fallback_title);

            let declared = parsed.frontmatter.object_type.as_deref();
            let inferred = infer_type(&format!("/{relative_path}"), declared);
            let target = suggested_target(&inferred, declared);
            let source_locator = scalar_extra(&parsed.frontmatter, "source_url")
                .or_else(|| scalar_extra(&parsed.frontmatter, "source"))
                .or_else(|| scalar_extra(&parsed.frontmatter, "source_path"));

            for capture in wikilink_pattern.captures_iter(&parsed.body) {
                if let Some(target) = capture.get(1) {
                    wikilinks_by_path
                        .entry(absolute_path.clone())
                        .or_default()
                        .push(target.as_str().trim().to_string());
                }
            }
            hashes
                .entry(hash.clone())
                .or_default()
                .push(absolute_path.clone());
            if let Some(source) = &source_locator {
                sources
                    .entry(source.trim().to_lowercase())
                    .or_default()
                    .push(absolute_path.clone());
            }
            let normalized = normalized_title(&title);
            if !normalized.is_empty() {
                titles
                    .entry(normalized)
                    .or_default()
                    .push(absolute_path.clone());
            }

            files.push(AuditedKnowledgeFile {
                path: absolute_path,
                root: slash_path(root),
                relative_path,
                byte_length: bytes.len() as u64,
                content_hash: hash,
                title,
                declared_type: declared.map(str::to_string),
                inferred_type: inferred,
                object_id: parsed.frontmatter.id,
                source_locator,
                suggested_target: target,
                warnings,
            });
        }
    }

    files.sort_by(|left, right| left.path.cmp(&right.path));
    let mut broken_wikilinks = BTreeMap::new();
    for (path, links) in wikilinks_by_path {
        let missing: Vec<String> = links
            .into_iter()
            .filter(|target| !known_titles.contains(target))
            .collect();
        if !missing.is_empty() {
            broken_wikilinks.insert(path, missing);
        }
    }
    let mut suggested_target_counts = BTreeMap::new();
    for file in &files {
        *suggested_target_counts
            .entry(file.suggested_target.clone())
            .or_insert(0) += 1;
    }

    KnowledgeAuditReport {
        schema_version: 1,
        scanned_at,
        read_only: true,
        roots: roots.iter().map(|root| slash_path(root)).collect(),
        file_count: files.len(),
        files,
        exact_duplicates: group_duplicates(hashes),
        source_duplicates: group_duplicates(sources),
        same_title_candidates: group_duplicates(titles),
        broken_wikilinks,
        read_errors,
        suggested_target_counts,
    }
}

#[tauri::command]
pub fn knowledge_audit_run() -> Result<serde_json::Value, String> {
    let roots = vec![crate::notebook::get_notes_dir(), crate::get_wiki_dir()];
    serde_json::to_value(audit_roots(&roots))
        .map_err(|error| format!("Could not serialize knowledge audit: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_fixture(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut file = fs::File::create(path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn audit_detects_cross_directory_duplicates_without_writing() {
        let root = std::env::temp_dir().join(format!("bob-audit-{}", ulid::Ulid::new()));
        let notes = root.join("notes");
        let wiki = root.join("wiki");
        let content = "# Same article\n\nUseful body\n";
        write_fixture(&notes.join("topics/article.md"), content);
        write_fixture(&wiki.join("sources/article.md"), content);

        let before_notes = fs::read(notes.join("topics/article.md")).unwrap();
        let before_wiki = fs::read(wiki.join("sources/article.md")).unwrap();
        let report = audit_roots(&[notes.clone(), wiki.clone()]);

        assert_eq!(report.file_count, 2);
        assert_eq!(report.exact_duplicates.len(), 1);
        assert_eq!(
            fs::read(notes.join("topics/article.md")).unwrap(),
            before_notes
        );
        assert_eq!(
            fs::read(wiki.join("sources/article.md")).unwrap(),
            before_wiki
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn audit_classifies_legacy_learned_types() {
        let root = std::env::temp_dir().join(format!("bob-audit-{}", ulid::Ulid::new()));
        write_fixture(
            &root.join("wiki/learned/feedback_test.md"),
            "---\ntype: feedback\ntitle: Correction\n---\n\nDo not repeat this.\n",
        );
        let report = audit_roots(&[root.join("wiki")]);
        assert_eq!(report.files[0].inferred_type, "legacy_memory");
        assert_eq!(
            report.files[0].suggested_target,
            "wiki/memories/corrections"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn audit_reports_broken_wikilinks() {
        let root = std::env::temp_dir().join(format!("bob-audit-{}", ulid::Ulid::new()));
        write_fixture(
            &root.join("notes/topics/linked.md"),
            "# Linked\n\nSee [[Missing Entity]].\n",
        );
        let report = audit_roots(&[root.join("notes")]);
        assert_eq!(report.broken_wikilinks.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn repository_fixture_produces_stable_read_only_report() {
        let fixture =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/knowledge_migration");
        let notes = fixture.join("notes");
        let wiki = fixture.join("wiki");
        let duplicate_note = notes.join("topics/shared-article.md");
        let duplicate_source = wiki.join("sources/shared-article.md");
        let before_note = fs::read(&duplicate_note).unwrap();
        let before_source = fs::read(&duplicate_source).unwrap();

        let first = audit_roots(&[notes.clone(), wiki.clone()]);
        let second = audit_roots(&[notes, wiki]);

        assert_eq!(first.file_count, 4);
        assert_eq!(first.exact_duplicates.len(), 1);
        assert_eq!(first.broken_wikilinks.len(), 1);
        assert_eq!(
            first.suggested_target_counts,
            second.suggested_target_counts
        );
        assert_eq!(fs::read(duplicate_note).unwrap(), before_note);
        assert_eq!(fs::read(duplicate_source).unwrap(), before_source);
    }

    #[test]
    #[ignore = "requires explicit BOB_AUDIT_NOTES_ROOT and BOB_AUDIT_WIKI_ROOT"]
    fn real_data_dry_run_prints_summary_without_writing() {
        let notes = PathBuf::from(
            std::env::var("BOB_AUDIT_NOTES_ROOT").expect("BOB_AUDIT_NOTES_ROOT is required"),
        );
        let wiki = PathBuf::from(
            std::env::var("BOB_AUDIT_WIKI_ROOT").expect("BOB_AUDIT_WIKI_ROOT is required"),
        );
        let report = audit_roots(&[notes, wiki]);
        let summary = serde_json::json!({
            "read_only": report.read_only,
            "file_count": report.file_count,
            "exact_duplicate_groups": report.exact_duplicates.len(),
            "source_duplicate_groups": report.source_duplicates.len(),
            "same_title_groups": report.same_title_candidates.len(),
            "broken_wikilink_files": report.broken_wikilinks.len(),
            "read_errors": report.read_errors,
            "suggested_target_counts": report.suggested_target_counts,
        });
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
        assert!(report.read_only);
    }
}
