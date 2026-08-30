//! MCP server — four tools over stdio.
//!
//! Each call opens its own SQLite connection. The index is read-only at query
//! time and SQLite handles concurrent readers, so this avoids threading a
//! connection through an async handler for no benefit.

use crate::config::NormalizeRules;
use crate::normalize::normalize;
use crate::store::{Hit, Store};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::ErrorData;
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct XsymServer {
    db: PathBuf,
    rules: NormalizeRules,
}

fn internal(e: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(e.to_string(), None)
}

fn invalid(msg: impl Into<std::borrow::Cow<'static, str>>) -> ErrorData {
    ErrorData::invalid_params(msg, None)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindSymbolArgs {
    /// Symbol name in any casing — `ConsumerConfig`, `consumer_config` and
    /// `jsConsumerConfig` are equivalent.
    pub name: String,
    /// Restrict to one kind: type, function, method, field, const.
    pub kind: Option<String>,
    /// Restrict to one language: go, rust, python.
    pub language: Option<String>,
    /// Restrict to one configured repo.
    pub repo: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CompareTypeArgs {
    /// Type name in any casing.
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchCodeArgs {
    /// Regular expression, passed to ripgrep.
    pub pattern: String,
    /// Optional glob filter, e.g. `*.go`.
    pub glob: Option<String>,
    /// Restrict the search to one configured repo.
    pub repo: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadFileArgs {
    /// Configured repo name.
    pub repo: String,
    /// Path relative to the repo root, as reported by find_symbol.
    pub path: String,
    /// First line to return, 1-indexed. Defaults to the start of the file.
    pub start_line: Option<u32>,
    /// Last line to return, inclusive. Defaults to 400 lines after the start.
    pub end_line: Option<u32>,
}

impl XsymServer {
    pub fn new(db: PathBuf, rules: NormalizeRules) -> Self {
        Self { db, rules }
    }

    fn store(&self) -> Result<Store, ErrorData> {
        Store::open(&self.db).map_err(internal)
    }
}

fn render_hits(query: &str, norm: &str, hits: &[Hit]) -> String {
    if hits.is_empty() {
        return format!("no symbol normalizes to `{norm}` (from `{query}`)");
    }
    let mut out = format!("`{query}` -> `{norm}` ({} hits)\n", hits.len());
    let mut language = String::new();
    for h in hits {
        if h.language != language {
            language = h.language.clone();
            out.push_str(&format!("\n{language}:\n"));
        }
        let parent = h.parent.as_ref().map(|p| format!("{p}.")).unwrap_or_default();
        out.push_str(&format!(
            "  {}/{}:{}  {}{} [{}]\n",
            h.repo, h.path, h.start_line, parent, h.name, h.kind
        ));
        if !h.signature.is_empty() {
            out.push_str(&format!("      {}\n", h.signature));
        }
    }
    out
}

#[tool_router]
impl XsymServer {
    /// Find where a symbol is declared, across every indexed language.
    /// Names are matched after normalization, so casing and language-specific
    /// prefixes do not matter.
    #[tool(
        description = "Find where a symbol is declared across every indexed language and repo. Matching is by normalized name, so ConsumerConfig, consumer_config and jsConsumerConfig all resolve to the same thing. Use this before search_code — it is exact and much faster."
    )]
    async fn find_symbol(
        &self,
        Parameters(args): Parameters<FindSymbolArgs>,
    ) -> Result<String, ErrorData> {
        let store = self.store()?;
        let norm = normalize(&args.name, &self.rules);
        let hits = store
            .find_filtered(
                &norm,
                args.kind.as_deref(),
                args.language.as_deref(),
                args.repo.as_deref(),
            )
            .map_err(internal)?;
        Ok(render_hits(&args.name, &norm, &hits))
    }

    /// Show one type side by side across languages, with its fields.
    #[tool(
        description = "Compare one type across languages: every declaration that normalizes to the same name, with the fields declared under each. Use this to check whether one language's struct is missing fields another has."
    )]
    async fn compare_type(
        &self,
        Parameters(args): Parameters<CompareTypeArgs>,
    ) -> Result<String, ErrorData> {
        let store = self.store()?;
        let norm = normalize(&args.name, &self.rules);

        let types = store
            .find_filtered(&norm, Some("type"), None, None)
            .map_err(internal)?;
        if types.is_empty() {
            return Ok(format!("no type normalizes to `{norm}` (from `{}`)", args.name));
        }

        let mut out = format!("`{}` -> `{norm}`\n", args.name);
        for t in &types {
            out.push_str(&format!(
                "\n{} · {}/{}:{}\n",
                t.language, t.repo, t.path, t.start_line
            ));
            // Fields are attributed to their enclosing type by name.
            let fields = store
                .fields_of(&t.name, &t.repo)
                .map_err(internal)?;
            if fields.is_empty() {
                out.push_str("  (no fields indexed)\n");
            }
            for f in fields {
                out.push_str(&format!("  {} [{}]\n", f.name, f.kind));
            }
        }
        Ok(out)
    }

    /// Full-text search. Delegates to ripgrep rather than duplicating an index.
    #[tool(
        description = "Regex search across indexed repos via ripgrep. Use this for comments, string literals, and call sites — anything the structural index does not carry. For finding a declaration, prefer find_symbol."
    )]
    async fn search_code(
        &self,
        Parameters(args): Parameters<SearchCodeArgs>,
    ) -> Result<String, ErrorData> {
        let store = self.store()?;
        let repos = store.repo_paths().map_err(internal)?;
        let roots: Vec<String> = match &args.repo {
            Some(name) => repos
                .iter()
                .filter(|(n, _)| n == name)
                .map(|(_, p)| p.clone())
                .collect(),
            None => repos.iter().map(|(_, p)| p.clone()).collect(),
        };
        if roots.is_empty() {
            return Err(invalid("no such repo"));
        }

        let mut cmd = std::process::Command::new("rg");
        cmd.arg("--line-number")
            .arg("--no-heading")
            .arg("--max-count=5")
            .arg("--max-columns=200")
            .arg("--color=never");
        if let Some(glob) = &args.glob {
            cmd.arg("--glob").arg(glob);
        }
        cmd.arg("--regexp").arg(&args.pattern);
        for root in &roots {
            cmd.arg(root);
        }

        let output = cmd.output().map_err(|e| {
            internal(format!("running ripgrep: {e} (is `rg` on PATH?)"))
        })?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() {
            return Ok(format!("no matches for `{}`", args.pattern));
        }

        // Trim absolute prefixes back to repo-relative for readability.
        let mut rendered = String::new();
        for line in stdout.lines().take(200) {
            let mut shown = line.to_string();
            for (name, path) in &repos {
                if let Some(rest) = line.strip_prefix(path.as_str()) {
                    shown = format!("{name}{}", rest.trim_start_matches('/'));
                    shown = format!("{name}/{}", shown.trim_start_matches(name));
                    break;
                }
            }
            rendered.push_str(&shown);
            rendered.push('\n');
        }
        Ok(rendered)
    }

    /// Read a slice of an indexed file.
    #[tool(
        description = "Read a range of lines from a file in an indexed repo. Paths are repo-relative, exactly as find_symbol reports them."
    )]
    async fn read_file(
        &self,
        Parameters(args): Parameters<ReadFileArgs>,
    ) -> Result<String, ErrorData> {
        let store = self.store()?;
        let root = store
            .repo_path(&args.repo)
            .map_err(internal)?
            .ok_or_else(|| invalid(format!("no repo named `{}`", args.repo)))?;

        // Refuse to escape the repo root.
        let candidate = Path::new(&root).join(&args.path);
        let root_canon = Path::new(&root).canonicalize().map_err(internal)?;
        let canon = candidate
            .canonicalize()
            .map_err(|_| invalid(format!("no such file: {}", args.path)))?;
        if !canon.starts_with(&root_canon) {
            return Err(invalid("path escapes the repo root"));
        }

        let text = std::fs::read_to_string(&canon).map_err(internal)?;
        let lines: Vec<&str> = text.lines().collect();
        let start = args.start_line.unwrap_or(1).max(1) as usize;
        let end = args
            .end_line
            .map(|e| e as usize)
            .unwrap_or(start + 399)
            .min(lines.len());
        if start > lines.len() {
            return Err(invalid(format!(
                "start_line {start} is past end of file ({} lines)",
                lines.len()
            )));
        }

        let mut out = format!("{}/{} lines {}-{}\n", args.repo, args.path, start, end);
        for (i, line) in lines[start - 1..end].iter().enumerate() {
            out.push_str(&format!("{:>6}  {}\n", start + i, line));
        }
        Ok(out)
    }
}

#[tool_handler(
    name = "xsym",
    instructions = "Structural cross-language code index. find_symbol locates a declaration by normalized name across languages; compare_type shows one type's fields side by side; search_code is regex fallback via ripgrep; read_file reads a slice of an indexed file. Prefer find_symbol over search_code when looking for a declaration."
)]
impl ServerHandler for XsymServer {}

/// Serve on stdio until the client disconnects.
pub async fn serve(db: PathBuf, rules: NormalizeRules) -> anyhow::Result<()> {
    let service = XsymServer::new(db, rules)
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
