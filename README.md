# xsym

A cross-language structural code index.

> **Status:** the CLI indexer works. The MCP server is **not built yet** —
> `src/mcp.rs` documents the intended surface but contains no code. Languages
> supported today: Go, Rust, Python.

Point it at a set of repositories in different languages and ask it where a
concept lives. The same struct spelled four ways across four languages
collapses to one lookup:

```
$ xsym find ConsumerConfig
`ConsumerConfig` -> `consumer_config` (14 hits)

go:
  nats.go/jetstream/consumer_config.go:103  ConsumerConfig [type]
      ConsumerConfig struct {
python:
  nats.py/nats/src/nats/js/api.py:576  ConsumerConfig [type]
      class ConsumerConfig(Base):
rust:
  nats.rs/nats/src/jetstream/types.rs:143  ConsumerConfig [type]
      pub struct ConsumerConfig {
```

## Why

If you maintain a protocol with clients in several languages, the same wire
type is declared once per language under a different naming convention. Keeping
them in sync is a manual diff across repos. Grep does not help, because the
names genuinely differ. This normalizes them and makes the comparison a query.

Nothing here is protocol-specific: the naming rules are configuration, so the
same binary works on any codebase with the same problem.

## Status

The indexing core is complete and tested. The MCP server is the next step —
see `src/mcp.rs`, which documents the four-tool surface it should expose but
implements none of it. Until that is written, this is a command-line tool only
and cannot be registered with an MCP client.

Working today:

```
xsym index          # walk configured repos, build the index
xsym find <name>    # cross-language lookup by normalized name
xsym stats          # row counts
```

## Configure

```toml
database = "xsym.db"

[[repos]]
name = "nats.go"
path = "/path/to/nats.go"

[normalize]
strip_prefixes = ["js", "nats"]   # jsConsumerConfig -> consumer_config
strip_suffixes = []

[normalize.synonyms]
configuration = "config"          # ConsumerConfiguration -> consumer_config
```

## Performance

Three repos, 552 files, 12,023 symbols: **7.1s** cold. Re-index with nothing
changed: **0.07s** — every file is content-hashed, so unchanged files are never
reparsed.

## Adding a language

Three steps, no Rust logic:

1. `cargo add tree-sitter-<lang>`
2. write `src/parse/queries/<lang>.scm`, capturing `@name` and `@def.<kind>`
3. add one arm to each match in `src/parse/mod.rs`

The extractor never names a language — it reads capture names, so the queries
carry all the per-language knowledge. See `DESIGN.md`.

## License

Apache-2.0
