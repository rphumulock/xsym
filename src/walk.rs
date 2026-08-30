//! File discovery. Honours .gitignore via the `ignore` crate, so we never
//! index vendored trees or build output.

use crate::parse::Language;
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

pub struct SourceFile {
    pub path: PathBuf,
    pub language: Language,
}

/// Walk `root`, yielding every file whose extension maps to a known language.
pub fn discover(root: &Path) -> Vec<SourceFile> {
    WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .build()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .filter_map(|e| {
            Language::from_path(e.path()).map(|language| SourceFile {
                path: e.path().to_path_buf(),
                language,
            })
        })
        .collect()
}
