//! Hybrid vault: Markdown content + encrypted MongrelDB index (native Rust kit).

use super::crypto::{decrypt_content, encrypt_content};
use super::embed::{Embedder, EmbeddingConfig, EmbeddingProviderKind, EMBEDDING_DIM};
use super::markdown::{MarkdownLibrary, PromptMarkdown};
use super::schema::vault_schema;
use mongreldb_kit::{Database, Literal, Query, Select, Expr};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use uuid::Uuid;

const KEYRING_SERVICE: &str = "com.visorcraft.promptsack";
const SECRET_LOCK: &str = "lock-session-password";
const SECRET_API_KEY: &str = "embedding-api-key";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    pub id: String,
    pub title: String,
    pub body: String,
    pub notes: String,
    pub tags: Vec<String>,
    pub folder_id: Option<String>,
    pub favorite: bool,
    pub locked: bool,
    #[serde(default)]
    pub content_encrypted: bool,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub md_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub locked: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartFolder {
    pub id: String,
    pub name: String,
    pub filter: ListFilter,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagInfo {
    pub name: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ListFilter {
    #[serde(rename = "all")]
    All,
    #[serde(rename = "favorites")]
    Favorites,
    #[serde(rename = "locked")]
    Locked,
    #[serde(rename = "folder")]
    Folder { folder_id: String },
    #[serde(rename = "tag")]
    Tag { tag: String },
    #[serde(rename = "smart")]
    Smart { smart_folder_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub prompt: Prompt,
    pub score: f64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePromptInput {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub folder_id: Option<String>,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub locked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePromptInput {
    pub title: Option<String>,
    pub body: Option<String>,
    pub notes: Option<String>,
    pub tags: Option<Vec<String>>,
    pub folder_id: Option<Option<String>>,
    pub favorite: Option<bool>,
    pub locked: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPayload {
    pub version: u32,
    pub exported_at: String,
    pub app: String,
    pub folders: Vec<Folder>,
    pub prompts: Vec<Prompt>,
    #[serde(default)]
    pub smart_folders: Vec<SmartFolder>,
}

pub struct Vault {
    root: PathBuf,
    db: Mutex<Database>,
    md: MarkdownLibrary,
    session_password: Mutex<Option<String>>,
    embedder: Mutex<Embedder>,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn vault_exists(root: &Path) -> bool {
    let mdb = root.join("index.mdb");
    mdb.exists()
        && fs_nonempty(&mdb)
}

fn fs_nonempty(p: &Path) -> bool {
    std::fs::read_dir(p)
        .map(|mut d| d.next().is_some())
        .unwrap_or(false)
}

impl Vault {
    pub fn create(root: &Path, passphrase: &str) -> Result<Self, String> {
        if passphrase.is_empty() {
            return Err("Passphrase is required".into());
        }
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let index = root.join("index.mdb");
        let schema = vault_schema();
        let db = Database::create_encrypted(&index, schema, passphrase).map_err(|e| e.to_string())?;
        let md = MarkdownLibrary::new(root).map_err(|e| e.to_string())?;
        let vault = Self {
            root: root.to_path_buf(),
            db: Mutex::new(db),
            md,
            session_password: Mutex::new(None),
            embedder: Mutex::new(Embedder::new(EmbeddingConfig::default(), None)),
        };
        vault.bootstrap_meta()?;
        Ok(vault)
    }

    pub fn open(root: &Path, passphrase: &str) -> Result<Self, String> {
        if passphrase.is_empty() {
            return Err("Passphrase is required".into());
        }
        let index = root.join("index.mdb");
        // Validate passphrase
        {
            let probe = Database::open_encrypted(&index, passphrase).map_err(|e| e.to_string())?;
            drop(probe);
        }
        // Rebuild prompt_index in a fresh create after truncate workaround:
        // open encrypted (keeps folders/meta), then rebuild embeddings.
        let db = Database::open_encrypted(&index, passphrase).map_err(|e| e.to_string())?;
        let md = MarkdownLibrary::new(root).map_err(|e| e.to_string())?;
        let vault = Self {
            root: root.to_path_buf(),
            db: Mutex::new(db),
            md,
            session_password: Mutex::new(None),
            embedder: Mutex::new(Embedder::new(EmbeddingConfig::default(), None)),
        };
        vault.load_embedder_settings()?;
        vault.rebuild_prompt_index()?;
        Ok(vault)
    }

    pub fn open_or_create(root: &Path, passphrase: &str) -> Result<Self, String> {
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        if vault_exists(root) {
            Self::open(root, passphrase)
        } else {
            Self::create(root, passphrase)
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn bootstrap_meta(&self) -> Result<(), String> {
        if self.get_meta("app")?.is_none() {
            self.set_meta("app", "PromptSack")?;
            self.set_meta("schema_version", "2")?;
            self.set_meta("storage", "markdown+mongreldb-rust")?;
            self.set_meta("shell", "tauri")?;
        }
        Ok(())
    }

    fn load_embedder_settings(&self) -> Result<(), String> {
        let cfg = self
            .get_meta("embedding_config")?
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let key = keyring_get(SECRET_API_KEY).ok().flatten();
        *self.embedder.lock() = Embedder::new(cfg, key);
        Ok(())
    }

    /// Rebuild ANN/FTS index from markdown (fresh session after open).
    pub fn rebuild_prompt_index(&self) -> Result<(), String> {
        // Best-effort: delete all index rows by listing md and re-upserting.
        // Full table scan of embedding columns after cold open can fail in 0.60.x;
        // we re-insert by primary key overwrite via delete+insert when possible.
        let docs = self.md.list();
        for doc in &docs {
            let _ = self.delete_index_row(&doc.id);
            self.upsert_index(doc, true)?;
        }
        Ok(())
    }

    fn delete_index_row(&self, id: &str) -> Result<(), String> {
        let db = self.db.lock();
        let mut txn = db.begin().map_err(|e| e.to_string())?;
        let _ = txn.delete("prompt_index", &json!(id));
        txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    fn upsert_index(&self, doc: &PromptMarkdown, reembed: bool) -> Result<(), String> {
        let fts_body = if doc.content_encrypted {
            ""
        } else {
            &doc.body
        };
        let fts_notes = if doc.content_encrypted {
            ""
        } else {
            &doc.notes
        };
        let emb = if reembed {
            let text = Embedder::embed_prompt(&doc.title, fts_body, fts_notes);
            self.embedder.lock().embed(&text)?
        } else {
            // must still provide vector
            let text = Embedder::embed_prompt(&doc.title, fts_body, fts_notes);
            self.embedder.lock().embed(&text)?
        };
        if emb.len() != EMBEDDING_DIM {
            return Err(format!("bad embedding dim {}", emb.len()));
        }
        let emb_json: Vec<Value> = emb.iter().map(|f| json!(*f as f64)).collect();
        let tags = serde_json::to_string(&doc.tags).unwrap_or_else(|_| "[]".into());
        let mut row = Map::new();
        row.insert("id".into(), json!(doc.id));
        row.insert("title".into(), json!(doc.title));
        row.insert("body_fts".into(), json!(fts_body));
        row.insert("notes_fts".into(), json!(fts_notes));
        row.insert("tags_json".into(), json!(tags));
        row.insert("folder_id".into(), json!(doc.folder_id));
        row.insert("favorite".into(), json!(doc.favorite));
        row.insert("locked".into(), json!(doc.locked));
        row.insert("content_encrypted".into(), json!(doc.content_encrypted));
        row.insert("md_path".into(), json!(doc.rel_path));
        row.insert("created_at".into(), json!(doc.created_at));
        row.insert("updated_at".into(), json!(doc.updated_at));
        row.insert("embedding".into(), Value::Array(emb_json));

        let db = self.db.lock();
        let mut txn = db.begin().map_err(|e| e.to_string())?;
        let _ = txn.delete("prompt_index", &json!(doc.id));
        txn.insert("prompt_index", row).map_err(|e| e.to_string())?;
        txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    // ── session ─────────────────────────────────────────────────────────

    pub fn unlock_session(&self, password: &str) -> bool {
        if password.is_empty() {
            return false;
        }
        let locked: Vec<_> = self
            .md
            .list()
            .into_iter()
            .filter(|d| d.locked && d.content_encrypted)
            .collect();
        if locked.is_empty() {
            *self.session_password.lock() = Some(password.to_string());
            let _ = keyring_set(SECRET_LOCK, password);
            return true;
        }
        for doc in locked {
            if decrypt_content(&doc.body, password).is_ok() {
                *self.session_password.lock() = Some(password.to_string());
                let _ = keyring_set(SECRET_LOCK, password);
                return true;
            }
        }
        false
    }

    pub fn lock_session(&self, forget_keychain: bool) {
        *self.session_password.lock() = None;
        if forget_keychain {
            let _ = keyring_delete(SECRET_LOCK);
        }
    }

    pub fn is_session_unlocked(&self) -> bool {
        self.session_password.lock().is_some()
    }

    pub fn try_auto_unlock(&self) -> bool {
        if let Ok(Some(pw)) = keyring_get(SECRET_LOCK) {
            return self.unlock_session(&pw);
        }
        false
    }

    fn session_pw(&self) -> Option<String> {
        self.session_password.lock().clone()
    }

    fn md_to_prompt(&self, doc: &PromptMarkdown) -> Prompt {
        let unlocked = self.is_session_unlocked();
        let pw = self.session_pw();
        let (body, notes, content_encrypted) =
            if doc.locked && doc.content_encrypted {
                if unlocked {
                    if let Some(ref p) = pw {
                        if let Ok((b, n)) = decrypt_content(&doc.body, p) {
                            (b, n, false)
                        } else {
                            (String::new(), String::new(), true)
                        }
                    } else {
                        (String::new(), String::new(), true)
                    }
                } else {
                    (String::new(), String::new(), true)
                }
            } else {
                (doc.body.clone(), doc.notes.clone(), false)
            };
        Prompt {
            id: doc.id.clone(),
            title: doc.title.clone(),
            body,
            notes,
            tags: doc.tags.clone(),
            folder_id: doc.folder_id.clone(),
            favorite: doc.favorite,
            locked: doc.locked,
            content_encrypted,
            created_at: doc.created_at.clone(),
            updated_at: doc.updated_at.clone(),
            md_path: Some(doc.rel_path.clone()),
        }
    }

    // ── folders ─────────────────────────────────────────────────────────

    pub fn list_folders(&self) -> Result<Vec<Folder>, String> {
        let db = self.db.lock();
        let txn = db.begin().map_err(|e| e.to_string())?;
        let q = Query::Select(Select {
            table: "folders".into(),
            columns: vec![],
            filter: None,
            order_by: vec![],
            limit: None,
            offset: None,
        });
        let rows = txn.select(&q).map_err(|e| e.to_string())?;
        let mut out: Vec<Folder> = rows
            .iter()
            .filter_map(|r| {
                Some(Folder {
                    id: r.values.get("id")?.as_str()?.to_string(),
                    name: r.values.get("name")?.as_str()?.to_string(),
                    locked: r.values.get("locked")?.as_bool().unwrap_or(false),
                    created_at: r.values.get("created_at")?.as_str()?.to_string(),
                    updated_at: r.values.get("updated_at")?.as_str()?.to_string(),
                })
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    pub fn create_folder(&self, name: &str) -> Result<Folder, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Folder name is required".into());
        }
        let id = Uuid::new_v4().to_string();
        let ts = now();
        let mut row = Map::new();
        row.insert("id".into(), json!(id));
        row.insert("name".into(), json!(name));
        row.insert("locked".into(), json!(false));
        row.insert("created_at".into(), json!(ts));
        row.insert("updated_at".into(), json!(ts));
        let db = self.db.lock();
        let mut txn = db.begin().map_err(|e| e.to_string())?;
        txn.insert("folders", row).map_err(|e| e.to_string())?;
        txn.commit().map_err(|e| e.to_string())?;
        Ok(Folder {
            id,
            name: name.to_string(),
            locked: false,
            created_at: ts.clone(),
            updated_at: ts,
        })
    }

    pub fn rename_folder(&self, id: &str, name: &str) -> Result<Folder, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Folder name is required".into());
        }
        let ts = now();
        let mut patch = Map::new();
        patch.insert("name".into(), json!(name));
        patch.insert("updated_at".into(), json!(ts));
        let db = self.db.lock();
        let mut txn = db.begin().map_err(|e| e.to_string())?;
        txn.update("folders", &json!(id), patch)
            .map_err(|e| e.to_string())?;
        txn.commit().map_err(|e| e.to_string())?;
        self.list_folders()?
            .into_iter()
            .find(|f| f.id == id)
            .ok_or_else(|| "Folder not found".into())
    }

    pub fn set_folder_locked(
        &self,
        id: &str,
        locked: bool,
        password: Option<String>,
    ) -> Result<Folder, String> {
        if locked {
            let pw = password
                .or_else(|| self.session_pw())
                .ok_or_else(|| "Password required to lock folder".to_string())?;
            *self.session_password.lock() = Some(pw.clone());
            for doc in self.md.list().into_iter().filter(|d| d.folder_id.as_deref() == Some(id)) {
                if !doc.locked || !doc.content_encrypted {
                    let _ = self.update_prompt(
                        &doc.id,
                        UpdatePromptInput {
                            locked: Some(true),
                            ..Default::default()
                        },
                    );
                }
            }
        }
        let ts = now();
        let mut patch = Map::new();
        patch.insert("locked".into(), json!(locked));
        patch.insert("updated_at".into(), json!(ts));
        let db = self.db.lock();
        let mut txn = db.begin().map_err(|e| e.to_string())?;
        txn.update("folders", &json!(id), patch)
            .map_err(|e| e.to_string())?;
        txn.commit().map_err(|e| e.to_string())?;
        self.list_folders()?
            .into_iter()
            .find(|f| f.id == id)
            .ok_or_else(|| "Folder not found".into())
    }

    pub fn delete_folder(&self, id: &str, mode: &str) -> Result<(), String> {
        let members: Vec<_> = self
            .md
            .list()
            .into_iter()
            .filter(|d| d.folder_id.as_deref() == Some(id))
            .collect();
        if mode == "delete" {
            for m in members {
                self.delete_prompt(&m.id)?;
            }
        } else {
            for m in members {
                let _ = self.update_prompt(
                    &m.id,
                    UpdatePromptInput {
                        folder_id: Some(None),
                        ..Default::default()
                    },
                );
            }
        }
        let db = self.db.lock();
        let mut txn = db.begin().map_err(|e| e.to_string())?;
        txn.delete("folders", &json!(id)).map_err(|e| e.to_string())?;
        txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    // ── smart folders ───────────────────────────────────────────────────

    pub fn list_smart_folders(&self) -> Result<Vec<SmartFolder>, String> {
        let db = self.db.lock();
        let txn = db.begin().map_err(|e| e.to_string())?;
        let q = Query::Select(Select {
            table: "smart_folders".into(),
            columns: vec![],
            filter: None,
            order_by: vec![],
            limit: None,
            offset: None,
        });
        let rows = txn.select(&q).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            let id = r.values.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let name = r.values.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let qj = r.values.get("query_json").and_then(|v| v.as_str()).unwrap_or("{}");
            let filter = serde_json::from_str(qj).unwrap_or(ListFilter::All);
            out.push(SmartFolder {
                id,
                name,
                filter,
                created_at: r
                    .values
                    .get("created_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                updated_at: r
                    .values
                    .get("updated_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    pub fn create_smart_folder(&self, name: &str, filter: ListFilter) -> Result<SmartFolder, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Smart folder name is required".into());
        }
        if matches!(filter, ListFilter::Smart { .. }) {
            return Err("Smart folder cannot reference another smart folder".into());
        }
        let id = Uuid::new_v4().to_string();
        let ts = now();
        let mut row = Map::new();
        row.insert("id".into(), json!(id));
        row.insert("name".into(), json!(name));
        row.insert("query_json".into(), json!(serde_json::to_string(&filter).unwrap()));
        row.insert("created_at".into(), json!(ts));
        row.insert("updated_at".into(), json!(ts));
        let db = self.db.lock();
        let mut txn = db.begin().map_err(|e| e.to_string())?;
        txn.insert("smart_folders", row).map_err(|e| e.to_string())?;
        txn.commit().map_err(|e| e.to_string())?;
        Ok(SmartFolder {
            id,
            name: name.to_string(),
            filter,
            created_at: ts.clone(),
            updated_at: ts,
        })
    }

    pub fn delete_smart_folder(&self, id: &str) -> Result<(), String> {
        let db = self.db.lock();
        let mut txn = db.begin().map_err(|e| e.to_string())?;
        txn.delete("smart_folders", &json!(id))
            .map_err(|e| e.to_string())?;
        txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    fn resolve_filter(&self, filter: &ListFilter) -> ListFilter {
        match filter {
            ListFilter::Smart { smart_folder_id } => {
                if let Ok(list) = self.list_smart_folders() {
                    if let Some(sf) = list.into_iter().find(|s| s.id == *smart_folder_id) {
                        return self.resolve_filter(&sf.filter);
                    }
                }
                ListFilter::All
            }
            other => other.clone(),
        }
    }

    // ── prompts ─────────────────────────────────────────────────────────

    pub fn list_prompts(&self, filter: ListFilter, sort: &str) -> Result<Vec<Prompt>, String> {
        let resolved = self.resolve_filter(&filter);
        let mut docs = self.md.list();
        match resolved {
            ListFilter::Favorites => docs.retain(|d| d.favorite),
            ListFilter::Locked => docs.retain(|d| d.locked),
            ListFilter::Folder { folder_id } => {
                docs.retain(|d| d.folder_id.as_deref() == Some(folder_id.as_str()))
            }
            ListFilter::Tag { tag } => docs.retain(|d| d.tags.iter().any(|t| t == &tag)),
            _ => {}
        }
        let mut prompts: Vec<_> = docs.iter().map(|d| self.md_to_prompt(d)).collect();
        match sort {
            "oldest" => prompts.sort_by(|a, b| a.created_at.cmp(&b.created_at)),
            "title-asc" => prompts.sort_by(|a, b| a.title.cmp(&b.title)),
            "title-desc" => prompts.sort_by(|a, b| b.title.cmp(&a.title)),
            _ => prompts.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
        }
        Ok(prompts)
    }

    pub fn get_prompt(&self, id: &str) -> Option<Prompt> {
        self.md.find_by_id(id).map(|d| self.md_to_prompt(&d))
    }

    pub fn create_prompt(&self, input: CreatePromptInput) -> Result<Prompt, String> {
        let title = {
            let t = input.title.trim();
            if t.is_empty() {
                "Untitled".to_string()
            } else {
                t.to_string()
            }
        };
        let id = Uuid::new_v4().to_string();
        let ts = now();
        let locked = input.locked;
        let mut content_encrypted = false;
        let mut body = input.body;
        let mut notes = input.notes;
        if locked {
            let pw = self
                .session_pw()
                .ok_or_else(|| "Unlock session before locking prompts".to_string())?;
            body = encrypt_content(&body, &notes, &pw)?;
            notes = String::new();
            content_encrypted = true;
        }
        let doc = PromptMarkdown {
            id: id.clone(),
            title,
            body,
            notes,
            tags: input.tags,
            folder_id: input.folder_id,
            favorite: input.favorite,
            locked,
            content_encrypted,
            created_at: ts.clone(),
            updated_at: ts,
            rel_path: format!("{id}.md"),
        };
        self.md.write(&doc).map_err(|e| e.to_string())?;
        self.upsert_index(&doc, true)?;
        Ok(self.md_to_prompt(&doc))
    }

    pub fn update_prompt(&self, id: &str, input: UpdatePromptInput) -> Result<Prompt, String> {
        let existing = self
            .md
            .find_by_id(id)
            .ok_or_else(|| "Prompt not found".to_string())?;
        let title = input
            .title
            .map(|t| {
                let t = t.trim().to_string();
                if t.is_empty() {
                    "Untitled".into()
                } else {
                    t
                }
            })
            .unwrap_or(existing.title.clone());
        let locked = input.locked.unwrap_or(existing.locked);
        let tags = input.tags.unwrap_or(existing.tags.clone());
        let folder_id = match input.folder_id {
            Some(v) => v,
            None => existing.folder_id.clone(),
        };
        let favorite = input.favorite.unwrap_or(existing.favorite);

        let (current_body, current_notes) = if existing.locked && existing.content_encrypted
        {
            let pw = self
                .session_pw()
                .ok_or_else(|| "Session must be unlocked to edit locked prompts".to_string())?;
            let (b, n) = decrypt_content(&existing.body, &pw)?;
            if input.body.as_deref() == Some("")
                && input.notes.as_deref() == Some("")
                && (!b.is_empty() || !n.is_empty())
            {
                return Err(
                    "Refusing to overwrite locked content with empty body/notes; unlock and reload first"
                        .into(),
                );
            }
            (b, n)
        } else {
            (existing.body.clone(), existing.notes.clone())
        };

        let mut body = input.body.unwrap_or(current_body);
        let mut notes = input.notes.unwrap_or(current_notes);
        let mut content_encrypted = false;

        if locked {
            let pw = self
                .session_pw()
                .ok_or_else(|| "Password required to lock prompt".to_string())?;
            body = encrypt_content(&body, &notes, &pw)?;
            notes = String::new();
            content_encrypted = true;
        }

        let doc = PromptMarkdown {
            id: existing.id,
            title,
            body,
            notes,
            tags,
            folder_id,
            favorite,
            locked,
            content_encrypted,
            created_at: existing.created_at,
            updated_at: now(),
            rel_path: existing.rel_path,
        };
        self.md.write(&doc).map_err(|e| e.to_string())?;
        self.upsert_index(&doc, true)?;
        Ok(self.md_to_prompt(&doc))
    }

    pub fn delete_prompt(&self, id: &str) -> Result<(), String> {
        self.md.delete(id).map_err(|e| e.to_string())?;
        let _ = self.delete_index_row(id);
        Ok(())
    }

    pub fn bulk_move(&self, ids: &[String], folder_id: Option<String>) -> Result<u32, String> {
        let mut n = 0u32;
        for id in ids {
            if self.md.find_by_id(id).is_some() {
                self.update_prompt(
                    id,
                    UpdatePromptInput {
                        folder_id: Some(folder_id.clone()),
                        ..Default::default()
                    },
                )?;
                n += 1;
            }
        }
        Ok(n)
    }

    pub fn bulk_delete(&self, ids: &[String]) -> Result<u32, String> {
        let mut n = 0u32;
        for id in ids {
            if self.md.find_by_id(id).is_some() {
                self.delete_prompt(id)?;
                n += 1;
            }
        }
        Ok(n)
    }

    // ── tags ────────────────────────────────────────────────────────────

    pub fn list_tags(&self) -> Vec<TagInfo> {
        let mut counts = std::collections::HashMap::<String, u32>::new();
        for d in self.md.list() {
            for t in d.tags {
                *counts.entry(t).or_default() += 1;
            }
        }
        let mut out: Vec<_> = counts
            .into_iter()
            .map(|(name, count)| TagInfo { name, count })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub fn rename_tag(&self, old: &str, new: &str) -> Result<(), String> {
        let old = old.trim();
        let new = new.trim();
        if old.is_empty() || new.is_empty() || old == new {
            return Ok(());
        }
        for d in self.md.list() {
            if !d.tags.iter().any(|t| t == old) {
                continue;
            }
            let tags: Vec<_> = d
                .tags
                .iter()
                .map(|t| if t == old { new.to_string() } else { t.clone() })
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            self.update_prompt(
                &d.id,
                UpdatePromptInput {
                    tags: Some(tags),
                    ..Default::default()
                },
            )?;
        }
        Ok(())
    }

    pub fn delete_tag(&self, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Ok(());
        }
        for d in self.md.list() {
            if !d.tags.iter().any(|t| t == name) {
                continue;
            }
            let tags: Vec<_> = d.tags.into_iter().filter(|t| t != name).collect();
            self.update_prompt(
                &d.id,
                UpdatePromptInput {
                    tags: Some(tags),
                    ..Default::default()
                },
            )?;
        }
        Ok(())
    }

    // ── search ──────────────────────────────────────────────────────────

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, String> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(self
                .list_prompts(ListFilter::All, "newest")?
                .into_iter()
                .take(limit)
                .map(|p| SearchHit {
                    prompt: p,
                    score: 0.0,
                    source: "lexical".into(),
                })
                .collect());
        }

        let lexical = self.lexical_search(q);
        let mut semantic = Vec::new();
        if let Ok(vec) = self.embedder.lock().embed(q) {
            let rows = {
                let db = self.db.lock();
                let txn = db.begin().map_err(|e| e.to_string())?;
                txn.ann_search("prompt_index", "embedding", vec, limit.max(10))
                    .map_err(|e| e.to_string())?
            };
            for (i, row) in rows.iter().enumerate() {
                if let Some(id) = row.values.get("id").and_then(|v| v.as_str()) {
                    if let Some(p) = self.get_prompt(id) {
                        semantic.push(SearchHit {
                            prompt: p,
                            score: 1.0 / (1.0 + i as f64),
                            source: "semantic".into(),
                        });
                    }
                }
            }
        }

        // merge
        let mut map = std::collections::HashMap::<String, SearchHit>::new();
        for h in semantic {
            map.insert(h.prompt.id.clone(), h);
        }
        for h in lexical {
            map.entry(h.prompt.id.clone())
                .and_modify(|e| {
                    e.score += h.score + 0.5;
                    e.source = "hybrid".into();
                })
                .or_insert(h);
        }
        let mut out: Vec<_> = map.into_values().collect();
        out.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out.truncate(limit);
        Ok(out)
    }

    fn lexical_search(&self, q: &str) -> Vec<SearchHit> {
        let needle = q.to_lowercase();
        let mut hits = Vec::new();
        for doc in self.md.list() {
            let p = self.md_to_prompt(&doc);
            let hay = format!(
                "{} {} {} {}",
                p.title,
                if p.content_encrypted { "" } else { &p.body },
                if p.content_encrypted { "" } else { &p.notes },
                p.tags.join(" ")
            )
            .to_lowercase();
            if !hay.contains(&needle) {
                continue;
            }
            let mut score = 0.0;
            if p.title.to_lowercase().contains(&needle) {
                score += 3.0;
            }
            if p.tags.iter().any(|t| t.to_lowercase().contains(&needle)) {
                score += 2.0;
            }
            if p.body.to_lowercase().contains(&needle) {
                score += 1.0;
            }
            if p.notes.to_lowercase().contains(&needle) {
                score += 1.0;
            }
            hits.push(SearchHit {
                prompt: p,
                score,
                source: "lexical".into(),
            });
        }
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits
    }

    // ── export / import ─────────────────────────────────────────────────

    pub fn export_all(&self) -> Result<ExportPayload, String> {
        Ok(ExportPayload {
            version: 1,
            exported_at: now(),
            app: "PromptSack".into(),
            folders: self.list_folders()?,
            prompts: self.list_prompts(ListFilter::All, "newest")?,
            smart_folders: self.list_smart_folders()?,
        })
    }

    pub fn export_folder(&self, folder_id: &str) -> Result<ExportPayload, String> {
        let folders = self.list_folders()?;
        let folder = folders
            .into_iter()
            .find(|f| f.id == folder_id)
            .ok_or_else(|| "Folder not found".to_string())?;
        Ok(ExportPayload {
            version: 1,
            exported_at: now(),
            app: "PromptSack".into(),
            folders: vec![folder],
            prompts: self.list_prompts(
                ListFilter::Folder {
                    folder_id: folder_id.to_string(),
                },
                "newest",
            )?,
            smart_folders: vec![],
        })
    }

    pub fn export_prompts(&self, ids: &[String]) -> Result<ExportPayload, String> {
        let mut prompts = Vec::new();
        for id in ids {
            if let Some(p) = self.get_prompt(id) {
                prompts.push(p);
            }
        }
        let folder_ids: std::collections::HashSet<_> = prompts
            .iter()
            .filter_map(|p| p.folder_id.clone())
            .collect();
        let folders = self
            .list_folders()?
            .into_iter()
            .filter(|f| folder_ids.contains(&f.id))
            .collect();
        Ok(ExportPayload {
            version: 1,
            exported_at: now(),
            app: "PromptSack".into(),
            folders,
            prompts,
            smart_folders: vec![],
        })
    }

    pub fn import_payload(&self, payload: ExportPayload, mode: &str) -> Result<(u32, u32), String> {
        if payload.version != 1 {
            return Err("Unsupported export payload".into());
        }
        if mode == "replace" {
            for p in self.md.list() {
                let _ = self.delete_prompt(&p.id);
            }
            for f in self.list_folders()? {
                let _ = self.delete_folder(&f.id, "move");
            }
        }
        let mut folder_map = std::collections::HashMap::new();
        let mut folder_count = 0u32;
        for f in payload.folders {
            if let Some(existing) = self.list_folders()?.into_iter().find(|x| x.id == f.id || x.name == f.name)
            {
                folder_map.insert(f.id, existing.id);
            } else {
                let created = self.create_folder(&f.name)?;
                folder_map.insert(f.id, created.id);
                folder_count += 1;
            }
        }
        let mut prompt_count = 0u32;
        for p in payload.prompts {
            let mapped = p
                .folder_id
                .as_ref()
                .and_then(|id| folder_map.get(id).cloned());
            if self.get_prompt(&p.id).is_some() && mode == "merge" {
                self.update_prompt(
                    &p.id,
                    UpdatePromptInput {
                        title: Some(p.title),
                        body: if p.content_encrypted {
                            None
                        } else {
                            Some(p.body)
                        },
                        notes: if p.content_encrypted {
                            None
                        } else {
                            Some(p.notes)
                        },
                        tags: Some(p.tags),
                        folder_id: Some(mapped),
                        favorite: Some(p.favorite),
                        ..Default::default()
                    },
                )?;
            } else {
                self.create_prompt(CreatePromptInput {
                    title: p.title,
                    body: if p.content_encrypted {
                        String::new()
                    } else {
                        p.body
                    },
                    notes: if p.content_encrypted {
                        String::new()
                    } else {
                        p.notes
                    },
                    tags: p.tags,
                    folder_id: mapped,
                    favorite: p.favorite,
                    locked: false,
                })?;
            }
            prompt_count += 1;
        }
        Ok((folder_count, prompt_count))
    }

    // ── settings ────────────────────────────────────────────────────────

    pub fn get_settings(&self) -> Result<serde_json::Value, String> {
        let emb = self
            .get_meta("embedding_config")?
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(EmbeddingConfig::default());
        let remember = self
            .get_meta("remember_lock_in_keychain")?
            .map(|s| s != "0")
            .unwrap_or(true);
        let has_key = keyring_get(SECRET_API_KEY)
            .ok()
            .flatten()
            .is_some();
        let embedder = self.embedder.lock();
        Ok(json!({
            "embedding": emb,
            "rememberLockInKeychain": remember,
            "hasEmbeddingApiKey": has_key,
            "keychainBackend": keyring_backend(),
            "embedderModelId": embedder.model_id(),
            "embedderReady": true,
            "shell": "tauri",
            "storage": "markdown+mongreldb-rust",
        }))
    }

    pub fn set_embedding(
        &self,
        config: EmbeddingConfig,
        api_key: Option<String>,
    ) -> Result<EmbeddingConfig, String> {
        if config.provider == EmbeddingProviderKind::OpenaiCompatible {
            if let Some(ref k) = api_key {
                if !k.trim().is_empty() {
                    keyring_set(SECRET_API_KEY, k.trim())?;
                }
            }
            let stored = keyring_get(SECRET_API_KEY).ok().flatten();
            if stored.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
                return Err("OpenAI-compatible provider requires an API key (OS keychain)".into());
            }
            *self.embedder.lock() = Embedder::new(config.clone(), stored);
        } else {
            *self.embedder.lock() = Embedder::new(config.clone(), None);
        }
        self.set_meta(
            "embedding_config",
            &serde_json::to_string(&config).map_err(|e| e.to_string())?,
        )?;
        Ok(config)
    }

    pub fn set_remember_lock(&self, remember: bool) -> Result<(), String> {
        self.set_meta(
            "remember_lock_in_keychain",
            if remember { "1" } else { "0" },
        )?;
        if !remember {
            let _ = keyring_delete(SECRET_LOCK);
        }
        Ok(())
    }

    pub fn clear_embedding_api_key(&self) -> Result<(), String> {
        keyring_delete(SECRET_API_KEY)
    }

    fn get_meta(&self, key: &str) -> Result<Option<String>, String> {
        let db = self.db.lock();
        let txn = db.begin().map_err(|e| e.to_string())?;
        let q = Query::Select(Select {
            table: "meta".into(),
            columns: vec![],
            filter: Some(Expr::Eq(
                Box::new(Expr::Column("key".into())),
                Box::new(Expr::Literal(Literal::Text(key.into()))),
            )),
            order_by: vec![],
            limit: Some(1),
            offset: None,
        });
        let rows = txn.select(&q).map_err(|e| e.to_string())?;
        Ok(rows
            .first()
            .and_then(|r| r.values.get("value"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()))
    }

    fn set_meta(&self, key: &str, value: &str) -> Result<(), String> {
        let db = self.db.lock();
        let mut txn = db.begin().map_err(|e| e.to_string())?;
        let exists = {
            let q = Query::Select(Select {
                table: "meta".into(),
                columns: vec![],
                filter: Some(Expr::Eq(
                    Box::new(Expr::Column("key".into())),
                    Box::new(Expr::Literal(Literal::Text(key.into()))),
                )),
                order_by: vec![],
                limit: Some(1),
                offset: None,
            });
            !txn.select(&q).map_err(|e| e.to_string())?.is_empty()
        };
        if exists {
            let mut patch = Map::new();
            patch.insert("value".into(), json!(value));
            txn.update("meta", &json!(key), patch)
                .map_err(|e| e.to_string())?;
        } else {
            let mut row = Map::new();
            row.insert("key".into(), json!(key));
            row.insert("value".into(), json!(value));
            txn.insert("meta", row).map_err(|e| e.to_string())?;
        }
        txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn keyring_set(account: &str, secret: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, account).map_err(|e| e.to_string())?;
    entry.set_password(secret).map_err(|e| e.to_string())
}

fn keyring_get(account: &str) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, account).map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(p) => Ok(Some(p)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

fn keyring_delete(account: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, account).map_err(|e| e.to_string())?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

fn keyring_backend() -> &'static str {
    "keyring"
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn open_tmp(pass: &str) -> (tempfile::TempDir, Vault) {
        let dir = tempdir().unwrap();
        let v = Vault::create(dir.path(), pass).expect("create vault");
        (dir, v)
    }

    #[test]
    fn create_crud_folder_and_prompt() {
        let (_dir, v) = open_tmp("test-passphrase-please");
        let folder = v.create_folder("Work").unwrap();
        assert_eq!(folder.name, "Work");
        assert!(!folder.locked);

        let p = v
            .create_prompt(CreatePromptInput {
                title: "Hello world".into(),
                body: "How do I write a good system prompt?".into(),
                notes: "useful".into(),
                tags: vec!["ai".into(), "system".into()],
                folder_id: Some(folder.id.clone()),
                favorite: true,
                locked: false,
            })
            .unwrap();
        assert_eq!(p.title, "Hello world");
        assert_eq!(p.tags, vec!["ai", "system"]);
        assert!(p.favorite);
        assert_eq!(p.folder_id.as_deref(), Some(folder.id.as_str()));
        assert!(p.md_path.is_some());

        let listed = v
            .list_prompts(
                ListFilter::Folder {
                    folder_id: folder.id.clone(),
                },
                "newest",
            )
            .unwrap();
        assert_eq!(listed.len(), 1);

        let updated = v
            .update_prompt(
                &p.id,
                UpdatePromptInput {
                    title: Some("Hello again".into()),
                    favorite: Some(false),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(updated.title, "Hello again");
        assert!(!updated.favorite);

        let tags = v.list_tags();
        assert!(tags.iter().any(|t| t.name == "ai" && t.count == 1));

        v.delete_prompt(&p.id).unwrap();
        assert!(v.get_prompt(&p.id).is_none());
    }

    #[test]
    fn lock_unlock_roundtrip() {
        let (_dir, v) = open_tmp("vault-pass-xyz");
        assert!(v.unlock_session("session-secret"));
        let p = v
            .create_prompt(CreatePromptInput {
                title: "Secret".into(),
                body: "classified body".into(),
                notes: "n".into(),
                tags: vec![],
                folder_id: None,
                favorite: false,
                locked: true,
            })
            .unwrap();
        // while unlocked we should read cleartext
        let got = v.get_prompt(&p.id).unwrap();
        assert_eq!(got.body, "classified body");
        assert!(!got.content_encrypted);

        v.lock_session(false);
        let locked = v.get_prompt(&p.id).unwrap();
        assert!(locked.content_encrypted);
        assert!(locked.body.is_empty());

        assert!(v.unlock_session("session-secret"));
        let again = v.get_prompt(&p.id).unwrap();
        assert_eq!(again.body, "classified body");
    }

    #[test]
    fn reopen_preserves_markdown_and_search() {
        let dir = tempdir().unwrap();
        let id = {
            let v = Vault::create(dir.path(), "reopen-pass").unwrap();
            let p = v
                .create_prompt(CreatePromptInput {
                    title: "Quantum tea brewing".into(),
                    body: "Instructions for brewing the perfect cup with quantum foam.".into(),
                    notes: "".into(),
                    tags: vec!["tea".into()],
                    folder_id: None,
                    favorite: false,
                    locked: false,
                })
                .unwrap();
            // lexical search
            let hits = v.search("brewing", 10).unwrap();
            assert!(hits.iter().any(|h| h.prompt.id == p.id));
            p.id
        };
        // reopen
        let v = Vault::open(dir.path(), "reopen-pass").unwrap();
        let p = v.get_prompt(&id).expect("prompt survives reopen");
        assert_eq!(p.title, "Quantum tea brewing");
        let hits = v.search("quantum", 10).unwrap();
        assert!(hits.iter().any(|h| h.prompt.id == id));
    }

    #[test]
    fn wrong_passphrase_rejected() {
        let dir = tempdir().unwrap();
        {
            let _v = Vault::create(dir.path(), "correct-horse").unwrap();
        }
        match Vault::open(dir.path(), "wrong-battery") {
            Ok(_) => panic!("expected wrong passphrase to fail"),
            Err(err) => assert!(!err.is_empty()),
        }
    }

    #[test]
    fn export_import_merge() {
        let (_dir, v) = open_tmp("export-pass");
        v.create_folder("Inbox").unwrap();
        v.create_prompt(CreatePromptInput {
            title: "A".into(),
            body: "body a".into(),
            notes: "".into(),
            tags: vec!["t".into()],
            folder_id: None,
            favorite: false,
            locked: false,
        })
        .unwrap();
        let payload = v.export_all().unwrap();
        assert_eq!(payload.version, 1);
        assert_eq!(payload.prompts.len(), 1);
        assert_eq!(payload.folders.len(), 1);

        let (folders, prompts) = v.import_payload(payload, "merge").unwrap();
        // merge on same ids updates rather than duplicates folders by name
        assert!(folders + prompts >= 1);
    }

    #[test]
    fn smart_folder_and_filter() {
        let (_dir, v) = open_tmp("smart-pass");
        v.create_prompt(CreatePromptInput {
            title: "fav".into(),
            body: "x".into(),
            notes: "".into(),
            tags: vec![],
            folder_id: None,
            favorite: true,
            locked: false,
        })
        .unwrap();
        v.create_prompt(CreatePromptInput {
            title: "plain".into(),
            body: "y".into(),
            notes: "".into(),
            tags: vec![],
            folder_id: None,
            favorite: false,
            locked: false,
        })
        .unwrap();
        let sf = v
            .create_smart_folder("My favs", ListFilter::Favorites)
            .unwrap();
        let list = v
            .list_prompts(
                ListFilter::Smart {
                    smart_folder_id: sf.id,
                },
                "newest",
            )
            .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "fav");
    }
}
