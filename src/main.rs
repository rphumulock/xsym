mod config;
mod extract;
mod mcp;
mod normalize;
mod parse;
mod store;
mod walk;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::Config;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "xsym", about = "Cross-language structural code index")]
struct Cli {
    /// Path to the TOML config.
    #[arg(short, long, default_value = "xsym.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Walk every configured repo and (re)build the index.
    Index,
    /// Look a symbol up by name, across every language.
    Find { name: String },
    /// Row counts for the current index.
    Stats,
    /// Run as an MCP server on stdio.
    Serve,
}

fn main() -> Result<()> {
    // stderr, always: in `serve` mode stdout is the MCP transport and any
    // stray line on it corrupts the protocol.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "xsym=info".into()),
        )
        .init();

    let cli = Cli::parse();
    let cfg = Config::load(&cli.config)?;
    let mut store = store::Store::open(&cfg.database)?;

    match cli.command {
        Command::Index => index(&cfg, &mut store),
        Command::Find { name } => find(&cfg, &store, &name),
        Command::Serve => {
            // stdout is the MCP transport, so logs must go to stderr only.
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(mcp::serve(cfg.database.clone(), cfg.normalize.clone()))
        }
        Command::Stats => {
            let (repos, files, symbols) = store.stats()?;
            println!("{repos} repos · {files} files · {symbols} symbols");
            Ok(())
        }
    }
}

fn index(cfg: &Config, store: &mut store::Store) -> Result<()> {
    for repo in &cfg.repos {
        let repo_id = store.upsert_repo(&repo.name, &repo.path.to_string_lossy())?;
        let files = walk::discover(&repo.path);

        let (mut indexed, mut skipped) = (0usize, 0usize);
        for file in files {
            let rel = file
                .path
                .strip_prefix(&repo.path)
                .unwrap_or(&file.path)
                .to_string_lossy()
                .to_string();

            let Ok(source) = std::fs::read_to_string(&file.path) else {
                continue; // not valid UTF-8; skip rather than fail the run
            };
            let hash = blake3::hash(source.as_bytes()).to_hex().to_string();

            // Incremental: unchanged files never get reparsed.
            if store.file_hash(repo_id, &rel)?.as_deref() == Some(hash.as_str()) {
                skipped += 1;
                continue;
            }

            let symbols = extract::extract(file.language, &source, &cfg.normalize)
                .with_context(|| format!("extracting {}", file.path.display()))?;
            store.replace_file(repo_id, &rel, file.language.name(), &hash, &symbols)?;
            indexed += 1;
        }

        println!("{}: {indexed} indexed, {skipped} unchanged", repo.name);
    }
    Ok(())
}

fn find(cfg: &Config, store: &store::Store, name: &str) -> Result<()> {
    let norm = normalize::normalize(name, &cfg.normalize);
    let hits = store.find_by_norm(&norm)?;

    if hits.is_empty() {
        println!("no symbol normalizes to `{norm}`");
        return Ok(());
    }

    println!("`{name}` -> `{norm}` ({} hits)\n", hits.len());
    let mut current_language = String::new();
    for hit in hits {
        if hit.language != current_language {
            current_language = hit.language.clone();
            println!("{current_language}:");
        }
        let parent = hit.parent.map(|p| format!("{p}.")).unwrap_or_default();
        println!(
            "  {}/{}:{}  {}{} [{}]",
            hit.repo, hit.path, hit.start_line, parent, hit.name, hit.kind
        );
        if !hit.signature.is_empty() {
            println!("      {}", hit.signature);
        }
    }
    Ok(())
}
