//! oasis-gen: generate API clients from OpenAPI 3.0, 3.1 or 3.2 documents.
//!
//! The pipeline is [`Documents`] (JSON/YAML files and the `$ref`s between
//! them) -> [`parse()`] (language-neutral [`ir::Api`]) -> an emitter for the
//! target language.

mod documents;
pub mod ir;
mod parse;
pub mod typescript;

pub use documents::Documents;
pub use parse::parse;

use std::fmt;

/// The language of the generated client. An emitter for another language
/// adds a variant here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    TypeScript,
}

/// Generates client source code from an OpenAPI 3.0, 3.1 or 3.2 document file in JSON or
/// YAML, following `$ref`s into other local files.
pub fn generate_file(path: &std::path::Path, lang: Lang) -> Result<String, Error> {
    convert(&Documents::from_file(path)?, lang)
}

/// Like [`generate_file`] for document text; relative `$ref`s to other files
/// resolve against the current directory.
pub fn generate(source: &str, lang: Lang) -> Result<String, Error> {
    convert(&Documents::from_text(source)?, lang)
}

/// Parses and emits, locating any error in its document.
fn convert(docs: &Documents, lang: Lang) -> Result<String, Error> {
    let api = parse(docs).map_err(|e| docs.locate(e))?;
    emit(&api, lang).map_err(|e| docs.locate(e))
}

fn emit(api: &ir::Api, lang: Lang) -> Result<String, Error> {
    match lang {
        Lang::TypeScript => typescript::emit(api),
    }
}

/// Parses JSON or YAML text into a JSON value. A syntax error carries its
/// line and column; the caller names the file.
pub fn load(source: &str) -> Result<serde_json::Value, Error> {
    // A byte order mark is not part of the document.
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let yaml = || serde_saphyr::from_str_with_options(source, yaml_options(source));
    if !source.trim_start().starts_with('{') {
        return yaml().map_err(yaml_error);
    }
    // Input starting with `{` may be JSON or non-JSON flow-style YAML. When
    // both parsers fail, the JSON error better describes the input.
    serde_json::from_str(source).or_else(|e| yaml().map_err(|_| json_error(e)))
}

/// Uses YAML 1.2 booleans only (`yes`/`no`/`on`/`off`/`y`/`n` stay strings),
/// and scales node and event budgets with the input so large documents load.
/// Alias expansion keeps its default limits.
pub(crate) fn yaml_options(source: &str) -> serde_saphyr::Options {
    let defaults = serde_saphyr::Budget::default();
    serde_saphyr::options! {
        strict_booleans: true,
        budget: serde_saphyr::budget! {
            max_nodes: defaults.max_nodes.max(source.len()),
            max_events: defaults.max_events.max(source.len()),
        },
    }
}

fn json_error(e: serde_json::Error) -> Error {
    let message = e.to_string();
    let suffix = format!(" at line {} column {}", e.line(), e.column());
    let message = message.strip_suffix(&suffix).unwrap_or(&message);
    // serde_json counts a position before a line's first character as column 0.
    Error::new(message).at(e.line(), e.column().max(1))
}

fn yaml_error(e: serde_saphyr::Error) -> Error {
    use serde_saphyr::budget::BudgetBreach;
    use serde_saphyr::{DefaultMessageFormatter, Error as E, MessageFormatter};
    // The error itself, without the snippet and alias wrappers.
    let mut inner = &e;
    while let E::WithSnippet { error, .. } | E::AliasError { error, .. } = inner {
        inner = error;
    }
    // Replace the messages that name the library's API or print its types.
    let message = match inner {
        E::Budget {
            breach: BudgetBreach::Depth { .. },
            ..
        } => format!(
            "nested more than {} levels deep",
            serde_saphyr::Budget::default().max_depth
        ),
        E::Budget { .. } => "the document is too large or too complex".into(),
        E::DuplicateMappingKey { key: Some(k), .. } => format!("duplicate key `{k}`"),
        E::DuplicateMappingKey { key: None, .. } => "duplicate key".into(),
        E::MultipleDocuments { .. } => "more than one YAML document".into(),
        _ => DefaultMessageFormatter.format_message(inner).into_owned(),
    };
    // The text can quote the document; keep control characters out of the terminal.
    let message: String = message
        .chars()
        .flat_map(|c| {
            let escaped = c.is_control().then(|| c.escape_default());
            escaped
                .into_iter()
                .flatten()
                .chain((!c.is_control()).then_some(c))
        })
        .collect();
    let error = Error::new(message);
    match e.location() {
        Some(l) if l.line() > 0 => error.at(l.line() as usize, l.column() as usize),
        _ => error,
    }
}

/// An error in a document or in reading it. Boxed, so that the `Result`s
/// returned through every level of a document's nesting stay small.
#[derive(Debug)]
pub struct Error {
    details: Box<Details>,
}

#[derive(Debug)]
struct Details {
    message: String,
    /// The file the error is in, if known.
    file: Option<String>,
    /// Line and column in that file, from 1.
    position: Option<(usize, usize)>,
    /// Address of the document value the error is about, until
    /// [`Documents::locate`] turns it into a file and position.
    value: Option<usize>,
    /// Key in [`ir::Api::schemas`] of the schema an emitter error is about.
    schema: Option<String>,
    /// Location in the document, outermost first.
    context: Vec<String>,
}

impl Error {
    pub(crate) fn new(message: impl Into<String>) -> Error {
        Error {
            details: Box::new(Details {
                message: message.into(),
                file: None,
                position: None,
                value: None,
                schema: None,
                context: Vec::new(),
            }),
        }
    }

    /// Records that an emitter error is about the schema with key `key`.
    pub(crate) fn about_schema(mut self, key: &str) -> Error {
        self.details.schema = Some(key.to_string());
        self
    }

    /// Records that the error is about document value `v`, unless a more
    /// specific value was recorded where the error originated.
    pub(crate) fn about(self, v: &serde_json::Value) -> Error {
        self.about_origin(ir::Origin::of(v))
    }

    /// Like [`Error::about`], for an emitter error about the IR node read
    /// from `origin`.
    pub(crate) fn about_origin(mut self, origin: ir::Origin) -> Error {
        if self.details.value.is_none() {
            self.details.value = origin.address();
        }
        self
    }

    pub(crate) fn at(mut self, line: usize, column: usize) -> Error {
        self.details.position = Some((line, column));
        self
    }

    pub(crate) fn in_file(mut self, file: impl Into<String>) -> Error {
        self.details.file = Some(file.into());
        self
    }

    /// Whether the error already names its file, as a syntax error does.
    pub(crate) fn is_placed(&self) -> bool {
        self.details.file.is_some()
    }

    /// Address of the document value the error is about, if recorded.
    pub(crate) fn value_address(&self) -> Option<usize> {
        self.details.value
    }

    /// Key of the schema an emitter error is about, if recorded.
    pub(crate) fn schema_key(&self) -> Option<&str> {
        self.details.schema.as_deref()
    }

    pub(crate) fn context(mut self, location: impl Into<String>) -> Error {
        self.details.context.insert(0, location.into());
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `file:line:column: `, the form editors and CI annotations recognize.
        match (&self.details.file, self.details.position) {
            (Some(file), Some((line, column))) => write!(f, "{file}:{line}:{column}: ")?,
            (Some(file), None) => write!(f, "{file}: ")?,
            (None, Some((line, column))) => write!(f, "{line}:{column}: ")?,
            (None, None) => {}
        }
        for c in &self.details.context {
            write!(f, "{c}: ")?;
        }
        f.write_str(&self.details.message)
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
        assert_eq!(err, "1:6: EOF while parsing a value");
        let err = load("a: [\n").unwrap_err().to_string();
        assert!(err.starts_with("1:4: "), "{err}");
        // A byte order mark does not hide JSON.
        assert!(load("\u{feff}{\"a\": 1}").is_ok());
    }

    #[test]
    fn loads_flow_style_yaml() {
        let v = load("{openapi: 3.1.0, tags: [a, yes]}\n").unwrap();
        assert_eq!(
            v,
            serde_json::json!({"openapi": "3.1.0", "tags": ["a", "yes"]})
        );
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
