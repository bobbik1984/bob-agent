use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const KNOWLEDGE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeObjectType {
    Source,
    Note,
    KnowledgePoint,
    Project,
    Entity,
    Memory,
    Session,
    Collection,
}

impl KnowledgeObjectType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Note => "note",
            Self::KnowledgePoint => "knowledge_point",
            Self::Project => "project",
            Self::Entity => "entity",
            Self::Memory => "memory",
            Self::Session => "session",
            Self::Collection => "collection",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "source" => Some(Self::Source),
            "note" => Some(Self::Note),
            "knowledge_point" => Some(Self::KnowledgePoint),
            "project" => Some(Self::Project),
            "entity" => Some(Self::Entity),
            "memory" => Some(Self::Memory),
            "session" => Some(Self::Session),
            "collection" => Some(Self::Collection),
            _ => None,
        }
    }

    pub fn id_prefix(self) -> &'static str {
        match self {
            Self::Source => "src",
            Self::Note => "note",
            Self::KnowledgePoint => "kp",
            Self::Project => "project",
            Self::Entity => "entity",
            Self::Memory => "memory",
            Self::Session => "session",
            Self::Collection => "collection",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeRelationType {
    DerivedFrom,
    Cites,
    BelongsTo,
    RelatedTo,
    Mentions,
    Supports,
    Contradicts,
    Supersedes,
    MergedInto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeRelation {
    pub relation: KnowledgeRelationType,
    pub target_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeFrontmatter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<u32>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub object_type: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default)]
    pub source_refs: Vec<String>,
    #[serde(default)]
    pub relations: Vec<KnowledgeRelation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KnowledgeMarkdown {
    pub frontmatter: KnowledgeFrontmatter,
    pub body: String,
    pub had_frontmatter: bool,
}

pub fn new_object_id(object_type: KnowledgeObjectType) -> String {
    format!("{}_{}", object_type.id_prefix(), ulid::Ulid::new())
}

pub fn parse_markdown(raw: &str) -> Result<KnowledgeMarkdown, String> {
    let normalized = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let mut lines = normalized.split_inclusive('\n');
    let first = match lines.next() {
        Some(line) => line.trim_end_matches(['\r', '\n']),
        None => "",
    };

    if first != "---" {
        return Ok(KnowledgeMarkdown {
            frontmatter: KnowledgeFrontmatter::default(),
            body: normalized.to_string(),
            had_frontmatter: false,
        });
    }

    let mut yaml = String::new();
    let mut body = String::new();
    let mut found_end = false;
    for line in lines {
        let marker = line.trim_end_matches(['\r', '\n']);
        if !found_end && marker == "---" {
            found_end = true;
            continue;
        }
        if found_end {
            body.push_str(line);
        } else {
            yaml.push_str(line);
        }
    }

    if !found_end {
        return Err("Markdown frontmatter is missing its closing delimiter".to_string());
    }

    let frontmatter = serde_yaml::from_str::<KnowledgeFrontmatter>(&yaml)
        .map_err(|error| format!("Invalid knowledge frontmatter: {error}"))?;

    Ok(KnowledgeMarkdown {
        frontmatter,
        body: body.trim_start_matches(['\r', '\n']).to_string(),
        had_frontmatter: true,
    })
}

pub fn validate_new_object(frontmatter: &KnowledgeFrontmatter) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    let object_type = match frontmatter
        .object_type
        .as_deref()
        .and_then(KnowledgeObjectType::parse)
    {
        Some(value) => value,
        None => {
            errors.push("type must be a supported knowledge object type".to_string());
            KnowledgeObjectType::Note
        }
    };

    if frontmatter.schema_version != Some(KNOWLEDGE_SCHEMA_VERSION) {
        errors.push(format!("schema_version must be {KNOWLEDGE_SCHEMA_VERSION}"));
    }
    match frontmatter.id.as_deref() {
        Some(id) if id.starts_with(&format!("{}_", object_type.id_prefix())) => {}
        Some(_) => errors.push(format!(
            "id must start with {}_ for type {:?}",
            object_type.id_prefix(),
            object_type
        )),
        None => errors.push("id is required".to_string()),
    }
    if frontmatter.title.trim().is_empty() {
        errors.push("title is required".to_string());
    }
    if let Some(confidence) = frontmatter.confidence {
        if !(0.0..=1.0).contains(&confidence) {
            errors.push("confidence must be between 0 and 1".to_string());
        }
    }
    if object_type != KnowledgeObjectType::Note && frontmatter.project_id.is_some() {
        errors.push("project_id is only a direct ownership field for notes".to_string());
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn render_markdown(document: &KnowledgeMarkdown) -> Result<String, String> {
    let yaml = serde_yaml::to_string(&document.frontmatter)
        .map_err(|error| format!("Could not serialize knowledge frontmatter: {error}"))?;
    let body = document.body.trim_start_matches(['\r', '\n']);
    Ok(format!("---\n{}---\n\n{}", yaml, body))
}

pub fn write_markdown_safely(path: &Path, document: &KnowledgeMarkdown) -> Result<(), String> {
    validate_new_object(&document.frontmatter).map_err(|errors| errors.join("; "))?;
    let rendered = render_markdown(document)?;
    let parent = path
        .parent()
        .ok_or_else(|| "Knowledge path has no parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create knowledge directory: {error}"))?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Knowledge path has an invalid file name".to_string())?;
    let temp_path = parent.join(format!(".{file_name}.{}.part", ulid::Ulid::new()));
    let backup_path = parent.join(format!(".{file_name}.{}.backup", ulid::Ulid::new()));

    let write_result = (|| -> Result<(), String> {
        let mut file = fs::File::create(&temp_path)
            .map_err(|error| format!("Could not create temporary knowledge file: {error}"))?;
        file.write_all(rendered.as_bytes())
            .map_err(|error| format!("Could not write temporary knowledge file: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Could not flush temporary knowledge file: {error}"))?;

        if path.exists() {
            fs::rename(path, &backup_path)
                .map_err(|error| format!("Could not stage existing knowledge file: {error}"))?;
        }

        if let Err(error) = fs::rename(&temp_path, path) {
            if backup_path.exists() {
                let _ = fs::rename(&backup_path, path);
            }
            return Err(format!("Could not commit knowledge file: {error}"));
        }

        if backup_path.exists() {
            fs::remove_file(&backup_path).map_err(|error| {
                format!("Knowledge was saved but backup cleanup failed: {error}")
            })?;
        }
        Ok(())
    })();

    if temp_path.exists() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result
}

pub fn suggested_relative_path(object_type: KnowledgeObjectType, id: &str, slug: &str) -> PathBuf {
    let directory = match object_type {
        KnowledgeObjectType::Source => "wiki/sources",
        KnowledgeObjectType::Note => "notes/topics",
        KnowledgeObjectType::KnowledgePoint => "wiki/knowledge",
        KnowledgeObjectType::Project | KnowledgeObjectType::Entity => "wiki/entities",
        KnowledgeObjectType::Memory => "wiki/memories",
        KnowledgeObjectType::Session => "wiki/sessions",
        KnowledgeObjectType::Collection => "wiki/collections",
    };
    PathBuf::from(directory).join(format!("{id}--{slug}.md"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_note() -> KnowledgeMarkdown {
        KnowledgeMarkdown {
            frontmatter: KnowledgeFrontmatter {
                id: Some("note_01JTEST".to_string()),
                schema_version: Some(KNOWLEDGE_SCHEMA_VERSION),
                object_type: Some(KnowledgeObjectType::Note.as_str().to_string()),
                title: "商业空间思考".to_string(),
                project_id: Some("project_01JABC".to_string()),
                ..KnowledgeFrontmatter::default()
            },
            body: "正文\n".to_string(),
            had_frontmatter: true,
        }
    }

    #[test]
    fn legacy_markdown_without_frontmatter_remains_readable() {
        let parsed = parse_markdown("# Old note\n\nBody").unwrap();
        assert!(!parsed.had_frontmatter);
        assert_eq!(parsed.body, "# Old note\n\nBody");
    }

    #[test]
    fn unknown_frontmatter_fields_survive_round_trip() {
        let raw = "---\nid: note_01JTEST\nschema_version: 1\ntype: note\ntitle: Test\nlegacy_key: keep-me\n---\n\nBody\n";
        let parsed = parse_markdown(raw).unwrap();
        assert_eq!(parsed.frontmatter.extra["legacy_key"], "keep-me");
        let rendered = render_markdown(&parsed).unwrap();
        let reparsed = parse_markdown(&rendered).unwrap();
        assert_eq!(reparsed.frontmatter.extra["legacy_key"], "keep-me");
        assert_eq!(reparsed.body, "Body\n");
    }

    #[test]
    fn validates_type_prefix_and_confidence() {
        let mut note = valid_note();
        note.frontmatter.id = Some("src_wrong".to_string());
        note.frontmatter.confidence = Some(1.5);
        let errors = validate_new_object(&note.frontmatter).unwrap_err();
        assert!(errors.iter().any(|error| error.contains("id must start")));
        assert!(errors.iter().any(|error| error.contains("confidence")));
    }

    #[test]
    fn only_notes_have_direct_project_ownership() {
        let mut source = valid_note();
        source.frontmatter.object_type = Some(KnowledgeObjectType::Source.as_str().to_string());
        source.frontmatter.id = Some("src_01JTEST".to_string());
        let errors = validate_new_object(&source.frontmatter).unwrap_err();
        assert!(errors.iter().any(|error| error.contains("project_id")));
    }

    #[test]
    fn safe_write_replaces_content_without_leaving_temp_files() {
        let root = std::env::temp_dir().join(format!("bob-knowledge-{}", ulid::Ulid::new()));
        let path = root.join("note.md");
        let mut note = valid_note();
        write_markdown_safely(&path, &note).unwrap();
        note.body = "更新后的正文\n".to_string();
        write_markdown_safely(&path, &note).unwrap();

        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("更新后的正文"));
        let leftovers = fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with('.'))
            .count();
        assert_eq!(leftovers, 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn suggested_paths_follow_portable_layout() {
        assert_eq!(
            suggested_relative_path(KnowledgeObjectType::KnowledgePoint, "kp_1", "客流"),
            PathBuf::from("wiki/knowledge/kp_1--客流.md")
        );
    }

    #[test]
    fn legacy_type_values_are_readable_but_not_valid_for_new_writes() {
        let raw = "---\ntype: feedback\ntitle: Old correction\n---\n\nBody\n";
        let parsed = parse_markdown(raw).unwrap();
        assert_eq!(parsed.frontmatter.object_type.as_deref(), Some("feedback"));
        assert!(validate_new_object(&parsed.frontmatter).is_err());
    }
}
