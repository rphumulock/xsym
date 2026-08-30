# xsym

A cross-language structural code index, served over MCP.

Point it at repositories in different languages and ask where a concept lives.
The same type spelled three ways collapses to one lookup:

```
$ xsym find ConsumerConfig
`ConsumerConfig` -> `consumer_config` (8 hits)

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

Grep cannot do this, because the names genuinely differ. xsym normalizes them
and makes the comparison a query.

**Languages today:** Go, Rust, Python. Adding one is a query file — see
[Adding a language](#adding-a-language).

---

## Quick start

### 1. Build and install

Needs a Rust toolchain. [ripgrep](https://github.com/BurntSushi/ripgrep) (`rg`)
must be on `PATH` for the `search_code` tool; everything else works without it.

```bash
git clone https://github.com/rphumulock/xsym
cd xsym
cargo build --release
cp target/release/xsym ~/.local/bin/     # or anywhere on your PATH
```

### 2. Write a config

Copy the example and point it at repos you already have checked out:

```bash
cp xsym.toml.example xsym.toml
```

```toml
database = "/home/you/.local/share/xsym/xsym.db"

[[repos]]
name = "nats.go"
path = "/home/you/src/nats.go"

[[repos]]
name = "nats.py"
path = "/home/you/src/nats.py"
```

One `[[repos]]` block per repository. `name` is yours to choose — it is what
shows up in results. Create the database's parent directory first:

```bash
mkdir -p ~/.local/share/xsym
```

### 3. Build the index

```bash
xsym -c xsym.toml index
```

```
nats.go: 182 indexed, 0 unchanged
nats.py: 198 indexed, 0 unchanged
```

Re-run it any time. Files are content-hashed, so unchanged files are never
reparsed and a repeat run takes well under a second.

### 4. Query it

```bash
xsym -c xsym.toml find StreamConfig
xsym -c xsym.toml stats
```

### 5. Wire it into an MCP client (optional)

```bash
claude mcp add xsym --scope user -- \
  ~/.local/bin/xsym -c /absolute/path/to/xsym.toml serve
```

Use absolute paths — the client starts the binary from an unspecified working
directory. Then start a new session; MCP servers connect at startup, so an
already-running session will not pick it up. Verify with `claude mcp list`.

### Skip the `-c` flag

Either run from the directory holding `xsym.toml` (the default), or alias it:

```bash
alias xsym='xsym -c ~/path/to/xsym.toml'
```

---

## Commands

| Command | What it does |
|---|---|
| `xsym index` | Walk the configured repos and build the index |
| `xsym find <name>` | Cross-language lookup by normalized name |
| `xsym stats` | Repo, file and symbol counts |
| `xsym serve` | Run as an MCP server on stdio |

`-c <path>` selects the config; it defaults to `./xsym.toml`.

## MCP tools

Four, deliberately no more:

| Tool | What it does |
|---|---|
| `find_symbol(name, kind?, language?, repo?)` | Declaration lookup by normalized name |
| `compare_type(name)` | One type across languages, with its fields |
| `search_code(pattern, glob?, repo?)` | Regex search via ripgrep |
| `read_file(repo, path, start_line?, end_line?)` | Read a slice of an indexed file |

`serve` speaks JSON-RPC on stdout, so all logging goes to stderr. Never print
to stdout from a tool handler.

## Tuning matches

If two things that should match don't, it's the normalization rules — not the
index. They live in the config:

```toml
[normalize]
# Leading tokens to drop: jsConsumerConfig -> consumer_config
strip_prefixes = ["js", "nats"]
# Trailing tokens to drop: ConsumerConfigOptions -> consumer_config
strip_suffixes = []

[normalize.synonyms]
# Token rewrites applied after splitting
configuration = "config"    # ConsumerConfiguration -> consumer_config
opts = "options"
```

Identifiers are split on separators, camelCase boundaries, and acronym
boundaries (`HTTPServer` -> `http`, `server`), then these rules apply. Check
what a name resolves to with `xsym find` — the output shows the normalized key.

Be careful with synonyms: a wrong one silently merges two distinct concepts.

## Performance

40 Go repositories, 2,441 files, 52,251 symbols:

| | |
|---|---|
| Cold index | ~25s |
| Re-index, nothing changed | **0.7s** |
| Database size | ~7 MB |

## Adding a language

Three steps, no changes to the extractor:

1. `cargo add tree-sitter-<lang>`
2. Write `src/parse/queries/<lang>.scm`, capturing `@name` and `@def.<kind>`:

   ```scheme
   (function_declaration name: (identifier) @name) @def.function
   ```

3. Add one arm to each `match` in `src/parse/mod.rs`

`extract.rs` never names a language — it reads capture names, so the queries
carry all the per-language knowledge. `DESIGN.md` explains why, and records the
trade-offs and what was deliberately left out.

## License

Apache-2.0
