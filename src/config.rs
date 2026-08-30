//! Configuration — repos to index and how to normalize identifiers.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    /// Where the SQLite index lives.
    #[serde(default = "default_db_path")]
    pub database: PathBuf,
    #[serde(default)]
    pub repos: Vec<RepoSpec>,
    #[serde(default)]
    pub normalize: NormalizeRules,
}

fn default_db_path() -> PathBuf {
    PathBuf::from("xsym.db")
}

#[derive(Debug, Deserialize, Clone)]
pub struct RepoSpec {
    pub name: String,
    pub path: PathBuf,
}

/// Domain-specific rules. These are what make the tool general: point it at a
/// different codebase and change this table, not the code.
#[derive(Debug, Deserialize, Default, Clone)]
pub struct NormalizeRules {
    /// Leading tokens to drop: `jsConsumerConfig` -> `consumer_config`.
    #[serde(default)]
    pub strip_prefixes: Vec<String>,
    /// Trailing tokens to drop.
    #[serde(default)]
    pub strip_suffixes: Vec<String>,
    /// Token rewrites applied after splitting: `configuration` -> `config`.
    #[serde(default)]
    pub synonyms: HashMap<String, String>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        let cfg: Config = toml::from_str(&text)
            .with_context(|| format!("parsing config {}", path.display()))?;
        Ok(cfg)
    }
}
