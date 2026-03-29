use std::path::PathBuf;

use crate::config::Config;

#[derive(Debug, Clone)]
pub struct AppContext {
    pub config: Config,
    pub workspace_root: PathBuf,
}

impl AppContext {
    pub fn new(config: Config) -> Self {
        let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            config,
            workspace_root,
        }
    }

    pub fn info(&self, message: impl AsRef<str>) {
        println!("{}", message.as_ref());
    }

    pub fn debug(&self, _message: impl AsRef<str>) {}
}
