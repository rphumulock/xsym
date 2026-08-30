# Design

## The pipeline

```
config.toml -> discover repos -> walk files (respecting .gitignore)
  -> tree-sitter parse -> run per-language query -> Symbol records
  -> normalize names -> SQLite -> query
```

## Decision 1 — tree-sitter queries are data, not code

The obvious implementation writes a syntax-tree visitor per language. That
grows linearly and gets long fast; the tool this one is modelled on carries a
2,800-line indexer.

Instead, each language gets a `.scm` query file using **consistent capture
names**:

```scheme
; queries/go.scm
(type_spec name: (type_identifier) @name) @def.type
(function_declaration name: (identifier) @name) @def.function
(method_declaration name: (field_identifier) @name) @def.method
(field_declaration name: (field_identifier) @name) @def.field
```

`extract.rs` reads `@def.<kind>` for the kind and `@name` for the name, and
never mentions a language. Adding one is a query file plus a dependency.

**Trade-off:** queries cannot express everything a visitor can. Python methods,
for instance, are indistinguishable from functions without ancestor inspection,
which is why `enclosing_type` walks parents separately. If a language needs
logic a query cannot express, that is the point to reconsider — not before.

## Decision 2 — normalization is configuration

`normalize.rs` splits identifiers on separators, camelCase boundaries, and
acronym boundaries (`HTTPServer` -> `http`, `server`), then applies
configured prefix stripping and synonyms.

Hardcoding one domain's conventions would make the tool useless elsewhere. The
rules live in TOML, so retargeting is an edit, not a fork.

**Trade-off:** the rules must be tuned per domain, and a bad synonym silently
merges distinct concepts. The `never_strips_to_empty` test guards the
degenerate case; the rest is the operator's judgement.

## Decision 3 — SQLite, with `norm` as the pivot

```sql
CREATE TABLE symbols (
  id INTEGER PRIMARY KEY,
  file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  name TEXT NOT NULL,      -- as written
  norm TEXT NOT NULL,      -- normalized key
  kind TEXT NOT NULL,
  parent TEXT,             -- enclosing type, for fields and methods
  start_line INTEGER NOT NULL,
  end_line INTEGER NOT NULL,
  signature TEXT
);
CREATE INDEX idx_symbols_norm ON symbols(norm);
```

Cross-language lookup is `WHERE norm = ?`. One index, one query.

Files carry a blake3 content hash, so re-indexing skips everything unchanged —
7.1s cold becomes 0.07s warm on the three-repo corpus.

`ON DELETE CASCADE` plus delete-then-insert per file means a file's symbols are
always consistent with its current contents; there is no partial-update path to
get wrong.

## Decision 4 — no full-text search

`search_code` should shell out to ripgrep. SQLite FTS5 would mean a second copy
of every source file in the database, kept in sync, to be worse at the job than
a tool already on the machine.

## Deliberately not built

- **Embeddings.** Normalization is doing this job deterministically and is
  debuggable when it is wrong. Revisit only if genuine synonyms — `Subscriber`
  vs `Consumer` — turn out to matter more than spelling variants.
- **Call graphs and references.** A different, much larger problem. This tool
  answers "where is this declared", not "what calls it".
- **An LSP.** Editors already solve within-language navigation. The value here
  is specifically *across* languages and repos.

## Next

The MCP server (`src/mcp.rs`). Four tools, no more:

| Tool | Backed by |
|---|---|
| `find_symbol(name, kind?, lang?, repo?)` | `Store::find_by_norm` |
| `compare_type(name)` | group hits by language, diff field sets |
| `search_code(pattern, glob?)` | ripgrep subprocess |
| `read_file(repo, path, range?)` | bounds-checked file read |
