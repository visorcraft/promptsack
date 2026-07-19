use crate::vault::Vault;

pub struct AppState {
    pub vault: Option<Vault>,
}

impl Default for AppState {
    fn default() -> Self {
        Self { vault: None }
    }
}
