//! MCP surface — the next thing to build.
//!
//! Deliberately empty for now. The indexing core below it is complete and
//! tested; wiring the server is a separate, well-defined step:
//!
//!   1. `cargo add rmcp --features server,transport-io`
//!   2. define a tool struct with `#[tool(tool_box)]`
//!   3. serve over stdio, register with `claude mcp add`
//!
//! Keep the surface to four tools. Every extra tool is another thing the model
//! has to choose between, and the index is already queryable four ways:
//!
//!   find_symbol(name, kind?, lang?, repo?)  -> Store::find_by_norm
//!   compare_type(name)                      -> group find_by_norm hits by language
//!   search_code(pattern, glob?)             -> shell out to ripgrep, do not
//!                                              reimplement full-text search
//!   read_file(repo, path, range?)           -> plain file read, bounds-checked
