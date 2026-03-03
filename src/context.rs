use std::path::PathBuf;

use crate::config::Config;

#[derive(Debug, Clone)]
pub struct AppContext {
    pub config: Config,
    pub verbose: bool,
    pub quiet: bool,
    pub workspace_root: PathBuf,
}

impl AppContext {
    pub fn new(config: Config, verbose: bool, quiet: bool) -> Self {
        let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            config,
            verbose,
            quiet,
            workspace_root,
        }
    }

    pub fn info(&self, message: impl AsRef<str>) {
        if !self.quiet {
            println!("{}", message.as_ref());
        }
    }

    pub fn debug(&self, message: impl AsRef<str>) {
        if self.verbose && !self.quiet {
            eprintln!("[debug] {}", message.as_ref());
        }
    }
}
