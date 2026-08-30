# xsym

A cross-language structural code index, served over MCP.

> **Status:** working. CLI and MCP server both run. Languages supported today:
> Go, Rust, Python.

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

Working. The indexer and the MCP server both run.

```
xsym index          # walk configured repos, build the index
xsym find <name>    # cross-language lookup by normalized name
xsym stats          # row counts
xsym serve          # run as an MCP server on stdio
```

## MCP

Register it with an MCP client:

```
claude mcp add xsym --scope user -- \
  /path/to/xsym -c /path/to/xsym.toml serve
```

Four tools, deliberately no more:

| Tool | What it does |
|---|---|
| `find_symbol(name, kind?, language?, repo?)` | Declaration lookup by normalized name, across languages |
| `compare_type(name)` | One type side by side across languages, with its fields |
| `search_code(pattern, glob?, repo?)` | Regex fallback via ripgrep |
| `read_file(repo, path, start_line?, end_line?)` | Read a slice of an indexed file |

`serve` speaks JSON-RPC on stdout, so all logging goes to stderr.

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
