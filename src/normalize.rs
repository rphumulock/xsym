//! Identifier normalization — the core idea.
//!
//! Every symbol name is reduced to a language-neutral key so that the same
//! concept spelled four ways across four languages collapses to one lookup:
//!
//! ```text
//! ConsumerConfig         -> consumer_config
//! consumer_config        -> consumer_config
//! ConsumerConfiguration  -> consumer_config   (synonym: configuration -> config)
//! jsConsumerConfig       -> consumer_config   (strip_prefixes = ["js"])
//! ```
//!
//! The rules are configuration, not code, so the same binary works on any
//! codebase — see `NormalizeRules` in `config.rs`.

use crate::config::NormalizeRules;

/// Split an identifier into lowercase tokens on separators, camelCase
/// boundaries, and acronym boundaries (`HTTPServer` -> `http`, `server`).
pub fn split_tokens(ident: &str) -> Vec<String> {
    let chars: Vec<char> = ident.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();

    for i in 0..chars.len() {
        let c = chars[i];

        if matches!(c, '_' | '-' | '.' | ' ' | ':') {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            continue;
        }

        if !cur.is_empty() {
            let prev = chars[i - 1];
            // lower|digit -> upper  ("consumerConfig")
            let camel = (prev.is_lowercase() || prev.is_numeric()) && c.is_uppercase();
            // upper -> upper followed by lower  ("HTTPServer" splits before "S")
            let acronym = prev.is_uppercase()
                && c.is_uppercase()
                && chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            if camel || acronym {
                out.push(std::mem::take(&mut cur));
            }
        }

        cur.extend(c.to_lowercase());
    }

    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Reduce an identifier to its normalized key under the given rules.
pub fn normalize(ident: &str, rules: &NormalizeRules) -> String {
    let mut tokens = split_tokens(ident);

    // Drop configured leading prefixes, but never reduce to nothing.
    while tokens.len() > 1 {
        let first = &tokens[0];
        if rules.strip_prefixes.iter().any(|p| p == first) {
            tokens.remove(0);
        } else {
            break;
        }
    }

    // Drop configured trailing suffixes, same guard.
    while tokens.len() > 1 {
        let last = tokens.last().expect("len > 1");
        if rules.strip_suffixes.iter().any(|s| s == last) {
            tokens.pop();
        } else {
            break;
        }
    }

    for t in tokens.iter_mut() {
        if let Some(canonical) = rules.synonyms.get(t.as_str()) {
            *t = canonical.clone();
        }
    }

    tokens.join("_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn rules() -> NormalizeRules {
        NormalizeRules {
            strip_prefixes: vec!["js".into(), "nats".into()],
            strip_suffixes: vec![],
            synonyms: HashMap::from([
                ("configuration".to_string(), "config".to_string()),
                ("opts".to_string(), "options".to_string()),
            ]),
        }
    }

    #[test]
    fn splits_every_convention() {
        assert_eq!(split_tokens("ConsumerConfig"), ["consumer", "config"]);
        assert_eq!(split_tokens("consumer_config"), ["consumer", "config"]);
        assert_eq!(split_tokens("consumerConfig"), ["consumer", "config"]);
        assert_eq!(split_tokens("consumer-config"), ["consumer", "config"]);
        assert_eq!(split_tokens("HTTPServer"), ["http", "server"]);
        assert_eq!(split_tokens("parseHTTP2Frame"), ["parse", "http2", "frame"]);
    }

    #[test]
    fn four_spellings_one_key() {
        let r = rules();
        let expected = "consumer_config";
        for spelling in [
            "ConsumerConfig",
            "consumer_config",
            "ConsumerConfiguration",
            "jsConsumerConfig",
        ] {
            assert_eq!(normalize(spelling, &r), expected, "failed on {spelling}");
        }
    }

    #[test]
    fn never_strips_to_empty() {
        let r = rules();
        assert_eq!(normalize("js", &r), "js");
        assert_eq!(normalize("nats", &r), "nats");
    }
}
