//! Pluggable embeddings: local MiniLM (fastembed) + OpenAI-compatible HTTP.

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use serde::{Deserialize, Serialize};
use std::sync::Mutex as StdMutex;

pub const EMBEDDING_DIM: usize = 384;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EmbeddingProviderKind {
    LocalMinilm,
    OpenaiCompatible,
}

impl Default for EmbeddingProviderKind {
    fn default() -> Self {
        Self::LocalMinilm
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingConfig {
    pub provider: EmbeddingProviderKind,
    #[serde(default = "default_base")]
    pub base_url: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_dim")]
    pub dimensions: usize,
}

fn default_base() -> String {
    "https://api.openai.com/v1".into()
}
fn default_model() -> String {
    "text-embedding-3-small".into()
}
fn default_dim() -> usize {
    EMBEDDING_DIM
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: EmbeddingProviderKind::LocalMinilm,
            base_url: default_base(),
            model: "Xenova/all-MiniLM-L6-v2".into(),
            dimensions: EMBEDDING_DIM,
        }
    }
}

pub struct Embedder {
    config: EmbeddingConfig,
    api_key: Option<String>,
    local: StdMutex<Option<TextEmbedding>>,
}

impl Embedder {
    pub fn new(config: EmbeddingConfig, api_key: Option<String>) -> Self {
        Self {
            config,
            api_key,
            local: StdMutex::new(None),
        }
    }

    pub fn model_id(&self) -> String {
        match self.config.provider {
            EmbeddingProviderKind::LocalMinilm => "all-MiniLM-L6-v2".into(),
            EmbeddingProviderKind::OpenaiCompatible => self.config.model.clone(),
        }
    }

    fn ensure_local(&self) -> Result<(), String> {
        let mut slot = self.local.lock().map_err(|e| e.to_string())?;
        if slot.is_none() {
            let model = TextEmbedding::try_new(
                InitOptions::new(EmbeddingModel::AllMiniLML6V2).with_show_download_progress(false),
            )
            .map_err(|e| format!("failed to load MiniLM: {e}"))?;
            *slot = Some(model);
        }
        Ok(())
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let input = if text.trim().is_empty() { " " } else { text };
        match self.config.provider {
            EmbeddingProviderKind::LocalMinilm => {
                self.ensure_local()?;
                let mut slot = self.local.lock().map_err(|e| e.to_string())?;
                let model = slot.as_mut().ok_or_else(|| "MiniLM not loaded".to_string())?;
                let out = model
                    .embed(vec![input.to_string()], None)
                    .map_err(|e| format!("embed failed: {e}"))?;
                let v = out
                    .into_iter()
                    .next()
                    .ok_or_else(|| "empty embedding".to_string())?;
                validate_dim(v)
            }
            EmbeddingProviderKind::OpenaiCompatible => {
                let key = self
                    .api_key
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| "OpenAI-compatible provider requires an API key".to_string())?;
                openai_embed(
                    &self.config.base_url,
                    &self.config.model,
                    key,
                    input,
                    EMBEDDING_DIM,
                )
            }
        }
    }

    pub fn embed_prompt(title: &str, body: &str, notes: &str) -> String {
        [title, body, notes]
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
            .chars()
            .take(8000)
            .collect()
    }
}

fn validate_dim(v: Vec<f32>) -> Result<Vec<f32>, String> {
    if v.len() != EMBEDDING_DIM {
        return Err(format!(
            "embedding dimension mismatch: expected {EMBEDDING_DIM}, got {}",
            v.len()
        ));
    }
    if v.iter().any(|x| !x.is_finite()) {
        return Err("embedding contains non-finite values".into());
    }
    Ok(v)
}

fn openai_embed(
    base_url: &str,
    model: &str,
    api_key: &str,
    text: &str,
    dimensions: usize,
) -> Result<Vec<f32>, String> {
    let base = base_url.trim_end_matches('/');
    let url = format!("{base}/embeddings");
    let client = reqwest::blocking::Client::new();
    let body = serde_json::json!({
        "model": model,
        "input": text,
        "dimensions": dimensions,
    });
    let res = client
        .post(&url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .map_err(|e| format!("HTTP error: {e}"))?;
    if !res.status().is_success() {
        let status = res.status();
        let t = res.text().unwrap_or_default();
        return Err(format!("OpenAI-compatible embeddings failed ({status}): {}", &t[..t.len().min(300)]));
    }
    let v: serde_json::Value = res.json().map_err(|e| e.to_string())?;
    let arr = v
        .pointer("/data/0/embedding")
        .and_then(|x| x.as_array())
        .ok_or_else(|| "response missing data[0].embedding".to_string())?;
    let mut out = Vec::with_capacity(arr.len());
    for n in arr {
        out.push(n.as_f64().ok_or_else(|| "non-numeric embedding".to_string())? as f32);
    }
    validate_dim(out)
}
