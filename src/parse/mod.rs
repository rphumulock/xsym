//! Language registry.
//!
//! Adding a language means three things and no Rust logic:
//!   1. add the `tree-sitter-<lang>` dependency
//!   2. drop a `queries/<lang>.scm` file next to the others
//!   3. add one arm to each match below
//!
//! The extractor never names a language — it reads `@def.<kind>` capture
//! names, so the queries carry all the per-language knowledge.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Go,
    Rust,
    Python,
}

impl Language {
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()? {
            "go" => Some(Language::Go),
            "rs" => Some(Language::Rust),
            "py" | "pyi" => Some(Language::Python),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Language::Go => "go",
            Language::Rust => "rust",
            Language::Python => "python",
        }
    }

    pub fn ts_language(&self) -> tree_sitter::Language {
        match self {
            Language::Go => tree_sitter_go::LANGUAGE.into(),
            Language::Rust => tree_sitter_rust::LANGUAGE.into(),
            Language::Python => tree_sitter_python::LANGUAGE.into(),
        }
    }

    pub fn query_source(&self) -> &'static str {
        match self {
            Language::Go => include_str!("queries/go.scm"),
            Language::Rust => include_str!("queries/rust.scm"),
            Language::Python => include_str!("queries/python.scm"),
        }
    }

    /// Node kinds that can enclose a field or method, used to attribute a
    /// symbol to its parent type.
    pub fn container_kinds(&self) -> &'static [&'static str] {
        match self {
            Language::Go => &["type_spec"],
            Language::Rust => &["struct_item", "enum_item", "trait_item", "impl_item"],
            Language::Python => &["class_definition"],
        }
    }
}
