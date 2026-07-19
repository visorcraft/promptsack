//! Prompt content as Markdown files (YAML frontmatter + body).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptMarkdown {
    pub id: String,
    pub title: String,
    pub body: String,
    pub notes: String,
    pub tags: Vec<String>,
    pub folder_id: Option<String>,
    pub favorite: bool,
    pub locked: bool,
    pub content_encrypted: bool,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip)]
    pub rel_path: String,
}

pub struct MarkdownLibrary {
    prompts_dir: PathBuf,
}

impl MarkdownLibrary {
    pub fn new(root: &Path) -> std::io::Result<Self> {
        let prompts_dir = root.join("prompts");
        fs::create_dir_all(&prompts_dir)?;
        Ok(Self { prompts_dir })
    }

    pub fn list(&self) -> Vec<PromptMarkdown> {
        let Ok(rd) = fs::read_dir(&self.prompts_dir) else {
            return vec![];
        };
        let mut out = Vec::new();
        for ent in rd.flatten() {
            let name = ent.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".md") {
                continue;
            }
            if let Ok(doc) = self.read(&name) {
                out.push(doc);
            }
        }
        out
    }

    pub fn find_by_id(&self, id: &str) -> Option<PromptMarkdown> {
        let direct = format!("{id}.md");
        if self.prompts_dir.join(&direct).exists() {
            return self.read(&direct).ok();
        }
        self.list().into_iter().find(|d| d.id == id)
    }

    pub fn read(&self, rel: &str) -> std::io::Result<PromptMarkdown> {
        let raw = fs::read_to_string(self.prompts_dir.join(rel))?;
        Ok(parse_markdown(&raw, rel))
    }

    pub fn write(&self, doc: &PromptMarkdown) -> std::io::Result<()> {
        let rel = if doc.rel_path.is_empty() {
            format!("{}.md", doc.id)
        } else {
            doc.rel_path.clone()
        };
        let mut d = doc.clone();
        d.rel_path = rel.clone();
        fs::write(self.prompts_dir.join(&rel), serialize_markdown(&d))
    }

    pub fn delete(&self, id: &str) -> std::io::Result<()> {
        let path = self.prompts_dir.join(format!("{id}.md"));
        if path.exists() {
            fs::remove_file(path)?;
        }
        for doc in self.list() {
            if doc.id == id {
                let p = self.prompts_dir.join(&doc.rel_path);
                if p.exists() {
                    let _ = fs::remove_file(p);
                }
            }
        }
        Ok(())
    }
}

fn yaml_escape(s: &str) -> String {
    if s.is_empty() {
        return "\"\"".into();
    }
    if s.chars()
        .any(|c| matches!(c, ':' | '#' | '{' | '}' | '[' | ']' | ',' | '&' | '*' | '!' | '|' | '>' | '\'' | '"' | '%' | '@' | '`' | '\n')
            || s.starts_with(' ')
            || s.ends_with(' '))
    {
        serde_json::to_string(s).unwrap_or_else(|_| format!("\"{s}\""))
    } else {
        s.to_string()
    }
}

fn parse_scalar(raw: &str) -> String {
    let t = raw.trim();
    if (t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')) {
        if let Ok(v) = serde_json::from_str::<String>(&format!("\"{}\"", &t[1..t.len() - 1])) {
            return v;
        }
        return t[1..t.len() - 1].to_string();
    }
    t.to_string()
}

pub fn parse_markdown(raw: &str, rel_path: &str) -> PromptMarkdown {
    let Some(rest) = raw.strip_prefix("---\n").or_else(|| raw.strip_prefix("---\r\n")) else {
        let id = rel_path.trim_end_matches(".md").to_string();
        return PromptMarkdown {
            id: id.clone(),
            title: id,
            body: raw.to_string(),
            notes: String::new(),
            tags: vec![],
            folder_id: None,
            favorite: false,
            locked: false,
            content_encrypted: false,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            rel_path: rel_path.to_string(),
        };
    };
    let (fm, body) = if let Some(i) = rest.find("\n---\n") {
        (&rest[..i], rest[i + 5..].trim_start_matches('\n').trim_end().to_string())
    } else if let Some(i) = rest.find("\n---\r\n") {
        (&rest[..i], rest[i + 6..].trim_start_matches('\n').trim_end().to_string())
    } else {
        (rest, String::new())
    };

    let mut fields = std::collections::HashMap::<String, String>::new();
    let mut tags = Vec::new();
    let mut in_tags = false;
    for line in fm.lines() {
        if in_tags {
            if let Some(item) = line.trim().strip_prefix("- ") {
                tags.push(parse_scalar(item));
                continue;
            }
            in_tags = false;
        }
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_string();
            let val = v.trim();
            if key == "tags" {
                if val.is_empty() {
                    in_tags = true;
                } else if val.starts_with('[') {
                    if let Ok(arr) = serde_json::from_str::<Vec<String>>(val) {
                        tags = arr;
                    }
                }
                continue;
            }
            fields.insert(key, parse_scalar(val));
        }
    }

    let bool_field = |k: &str| matches!(fields.get(k).map(|s| s.as_str()), Some("true" | "yes" | "1"));
    let folder = fields.get("folderId").cloned().filter(|s| s != "null" && !s.is_empty());

    PromptMarkdown {
        id: fields
            .get("id")
            .cloned()
            .unwrap_or_else(|| rel_path.trim_end_matches(".md").to_string()),
        title: fields.get("title").cloned().unwrap_or_else(|| "Untitled".into()),
        body,
        notes: fields.get("notes").cloned().unwrap_or_default(),
        tags,
        folder_id: folder,
        favorite: bool_field("favorite"),
        locked: bool_field("locked"),
        content_encrypted: bool_field("contentEncrypted"),
        created_at: fields
            .get("createdAt")
            .cloned()
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        updated_at: fields
            .get("updatedAt")
            .cloned()
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        rel_path: rel_path.to_string(),
    }
}

pub fn serialize_markdown(doc: &PromptMarkdown) -> String {
    let mut lines = vec!["---".to_string()];
    lines.push(format!("id: {}", yaml_escape(&doc.id)));
    lines.push(format!("title: {}", yaml_escape(&doc.title)));
    lines.push(format!("favorite: {}", if doc.favorite { "true" } else { "false" }));
    lines.push(format!("locked: {}", if doc.locked { "true" } else { "false" }));
    lines.push(format!(
        "contentEncrypted: {}",
        if doc.content_encrypted { "true" } else { "false" }
    ));
    lines.push(format!(
        "folderId: {}",
        doc.folder_id
            .as_ref()
            .map(|s| yaml_escape(s))
            .unwrap_or_else(|| "null".into())
    ));
    lines.push(format!("createdAt: {}", yaml_escape(&doc.created_at)));
    lines.push(format!("updatedAt: {}", yaml_escape(&doc.updated_at)));
    if !doc.content_encrypted && !doc.notes.is_empty() {
        lines.push(format!("notes: {}", yaml_escape(&doc.notes)));
    } else {
        lines.push("notes: \"\"".into());
    }
    if doc.tags.is_empty() {
        lines.push("tags: []".into());
    } else {
        lines.push("tags:".into());
        for t in &doc.tags {
            lines.push(format!("  - {}", yaml_escape(t)));
        }
    }
    lines.push("---".into());
    lines.push(String::new());
    lines.push(doc.body.clone());
    if !doc.body.ends_with('\n') {
        lines.push(String::new());
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let doc = PromptMarkdown {
            id: "a1".into(),
            title: "Hello: world".into(),
            body: "Line one\n\nLine two".into(),
            notes: "n".into(),
            tags: vec!["x".into(), "y".into()],
            folder_id: None,
            favorite: true,
            locked: false,
            content_encrypted: false,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-02T00:00:00Z".into(),
            rel_path: "a1.md".into(),
        };
        let raw = serialize_markdown(&doc);
        let parsed = parse_markdown(&raw, "a1.md");
        assert_eq!(parsed.title, "Hello: world");
        assert_eq!(parsed.body, "Line one\n\nLine two");
        assert_eq!(parsed.tags, vec!["x", "y"]);
        assert!(parsed.favorite);
    }
}
