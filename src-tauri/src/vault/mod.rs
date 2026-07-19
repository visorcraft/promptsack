//! PromptSack vault: Markdown content + encrypted MongrelDB index (Rust native).

pub mod crypto;
pub mod embed;
pub mod markdown;
pub mod schema;
pub mod store;

pub use store::Vault;
