//! Symbol extraction. Language-agnostic: it runs whatever query the language
//! registry hands it and reads the capture names.

use crate::config::NormalizeRules;
use crate::normalize::normalize;
use crate::parse::Language;
use anyhow::{Context, Result};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Parser, Query, QueryCursor};

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub norm: String,
    pub kind: String,
    pub parent: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub signature: String,
}

pub fn extract(language: Language, source: &str, rules: &NormalizeRules) -> Result<Vec<Symbol>> {
    let ts_lang = language.ts_language();

    let mut parser = Parser::new();
    parser
        .set_language(&ts_lang)
        .with_context(|| format!("loading {} grammar", language.name()))?;
    let tree = parser
        .parse(source, None)
        .context("tree-sitter returned no tree")?;

    let query = Query::new(&ts_lang, language.query_source())
        .with_context(|| format!("compiling {}.scm", language.name()))?;

    let capture_names = query.capture_names();
    let bytes = source.as_bytes();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), bytes);
    let mut out = Vec::new();

    while let Some(m) = matches.next() {
        let mut kind: Option<&str> = None;
        let mut def_node: Option<Node> = None;
        let mut name_node: Option<Node> = None;

        for cap in m.captures {
            let cap_name = capture_names[cap.index as usize];
            if let Some(k) = cap_name.strip_prefix("def.") {
                kind = Some(k);
                def_node = Some(cap.node);
            } else if cap_name == "name" {
                name_node = Some(cap.node);
            }
        }

        let (Some(kind), Some(def_node), Some(name_node)) = (kind, def_node, name_node) else {
            continue;
        };

        let name = name_node.utf8_text(bytes)?.to_string();
        let norm = normalize(&name, rules);

        out.push(Symbol {
            norm,
            kind: kind.to_string(),
            parent: enclosing_type(def_node, bytes, language),
            start_line: def_node.start_position().row + 1,
            end_line: def_node.end_position().row + 1,
            signature: first_line(def_node.utf8_text(bytes).unwrap_or(&name)),
            name,
        });
    }

    Ok(out)
}

/// Walk ancestors looking for the type that encloses this node, so a field
/// knows which struct it belongs to.
fn enclosing_type(node: Node, bytes: &[u8], language: Language) -> Option<String> {
    let containers = language.container_kinds();
    let mut current = node.parent();
    while let Some(n) = current {
        if containers.contains(&n.kind()) {
            if let Some(name) = n.child_by_field_name("name") {
                return name.utf8_text(bytes).ok().map(str::to_string);
            }
            // Rust `impl_item` names its type under `type`, not `name`.
            if let Some(ty) = n.child_by_field_name("type") {
                return ty.utf8_text(bytes).ok().map(str::to_string);
            }
        }
        current = n.parent();
    }
    None
}

fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default().trim();
    if line.len() > 200 {
        format!("{}…", &line[..200])
    } else {
        line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_go_struct_and_fields() {
        let src = r#"
package jetstream

type ConsumerConfig struct {
	Durable   string
	AckPolicy int
}

func (c *ConsumerConfig) Validate() error { return nil }
"#;
        let syms = extract(Language::Go, src, &NormalizeRules::default()).unwrap();

        let ty = syms.iter().find(|s| s.kind == "type").unwrap();
        assert_eq!(ty.name, "ConsumerConfig");
        assert_eq!(ty.norm, "consumer_config");

        let field = syms.iter().find(|s| s.name == "Durable").unwrap();
        assert_eq!(field.kind, "field");
        assert_eq!(field.parent.as_deref(), Some("ConsumerConfig"));

        assert!(syms.iter().any(|s| s.kind == "method" && s.name == "Validate"));
    }

    #[test]
    fn python_class_normalizes_to_the_same_key_as_go() {
        let go = extract(
            Language::Go,
            "package p\ntype ConsumerConfig struct {}\n",
            &NormalizeRules::default(),
        )
        .unwrap();
        let py = extract(
            Language::Python,
            "class consumer_config:\n    pass\n",
            &NormalizeRules::default(),
        )
        .unwrap();

        assert_eq!(go[0].norm, py[0].norm);
    }
}
