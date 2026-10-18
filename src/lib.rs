//! oasis-gen: generate API clients from OpenAPI 3.0, 3.1 or 3.2 documents.
//!
//! The pipeline is [`Documents`] (JSON/YAML files and the `$ref`s between
//! them) -> [`parse`] (language-neutral [`ir::Api`]) -> an emitter for the
//! target language.

mod documents;
pub mod ir;
mod parse;
pub mod typescript;

pub use documents::Documents;
pub use parse::parse;

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    TypeScript,
}

impl Lang {
    pub fn from_name(name: &str) -> Option<Lang> {
        match name {
            "typescript" | "ts" => Some(Lang::TypeScript),
            _ => None,
        }
    }
}

/// Generates client source code from an OpenAPI 3.0, 3.1 or 3.2 document file in JSON or
/// YAML, following `$ref`s into other local files.
pub fn generate_file(path: &std::path::Path, lang: Lang) -> Result<String, Error> {
    emit(&parse(&Documents::from_file(path)?)?, lang)
}

/// Like [`generate_file`] for document text; relative `$ref`s to other files
/// resolve against the current directory.
pub fn generate(source: &str, lang: Lang) -> Result<String, Error> {
    emit(&parse(&Documents::from_value(load(source)?)?)?, lang)
}

fn emit(api: &ir::Api, lang: Lang) -> Result<String, Error> {
    match lang {
        Lang::TypeScript => typescript::emit(api),
    }
}

/// Parses JSON or YAML text into a JSON value.
pub fn load(source: &str) -> Result<serde_json::Value, Error> {
    if source.trim_start().starts_with('{') {
        serde_json::from_str(source).map_err(|e| Error::new(format!("invalid JSON: {e}")))
    } else {
        // YAML 1.2 booleans only (`yes`/`no`/`on`/`off`/`y`/`n` stay strings), and
        // node/event budgets that scale with the input so large documents load.
        // Alias expansion keeps its default limits.
        let defaults = serde_saphyr::Budget::default();
        let options = serde_saphyr::options! {
            strict_booleans: true,
            budget: serde_saphyr::budget! {
                max_nodes: defaults.max_nodes.max(source.len()),
                max_events: defaults.max_events.max(source.len()),
            },
        };
        serde_saphyr::from_str_with_options(source, options)
            .map_err(|e| Error::new(format!("invalid YAML: {e}")))
    }
}

#[derive(Debug)]
pub struct Error {
    message: String,
    /// Location in the document, outermost first.
    context: Vec<String>,
}

impl Error {
    pub(crate) fn new(message: impl Into<String>) -> Error {
        Error {
            message: message.into(),
            context: Vec::new(),
        }
    }

    pub(crate) fn context(mut self, location: impl Into<String>) -> Error {
        self.context.insert(0, location.into());
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for c in &self.context {
            write!(f, "{c}: ")?;
        }
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_uses_core_schema_booleans() {
        let v = load("a: [yes, off, N, true]\n").unwrap();
        assert_eq!(v["a"], serde_json::json!(["yes", "off", "N", true]));
    }

    #[test]
    fn loads_json_and_reports_invalid_input() {
        assert_eq!(load(" {\"a\": [1]}").unwrap()["a"], serde_json::json!([1]));
        let err = load("{\"a\": ").unwrap_err().to_string();
        assert!(err.starts_with("invalid JSON: "), "{err}");
        let err = load("a: [\n").unwrap_err().to_string();
        assert!(err.starts_with("invalid YAML: "), "{err}");
    }

    #[test]
    fn large_yaml_documents_load() {
        // More nodes than serde-saphyr's default budget of 250,000.
        let source = format!("items:\n{}", "  - 1\n".repeat(300_000));
        assert_eq!(
            load(&source).unwrap()["items"].as_array().unwrap().len(),
            300_000
        );
    }
}
