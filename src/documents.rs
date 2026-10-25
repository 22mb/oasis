//! Loading OpenAPI documents and resolving URI references between them.
//!
//! Base URIs follow OpenAPI 3.2 and JSON Schema 2020-12: a document's `$self`
//! (resolved against its retrieval URI) and a schema's `$id` override the
//! retrieval URI. Documents are identified by `$self` and by retrieval URI;
//! schema resources by `$id`. Only local files are loaded: a URI under the
//! directory of some document's `$self` is mapped to the same relative
//! location under that document's file (`$id` directories are not mapped).
//!
//! All loaded documents are scanned for `$ref`, `$id` and `$anchor` up front.
//! The scan does not know which objects are schemas, so a `$id` inside an
//! example value is registered too, and references that cannot be loaded are
//! only reported when the parser actually follows them.

use crate::Error;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use url::Url;

pub struct Documents {
    docs: Vec<Document>,
    /// Absolute URI without fragment -> resource it identifies.
    resources: HashMap<String, Resource>,
    /// (resource URI, `$anchor`) -> JSON pointer in the resource's document.
    anchors: HashMap<(String, String), String>,
    /// URIs that could not be loaded, with the reason.
    failures: HashMap<String, String>,
}

struct Document {
    /// Canonical `file:` URL the document was read from.
    retrieval: Url,
    /// `$self` resolved against the retrieval URI, or the retrieval URI.
    base: Rc<Url>,
    value: Value,
    /// Schema resources (`$id`) inside the document: pointer and base URI.
    ids: Vec<(String, Rc<Url>)>,
}

#[derive(Clone)]
struct Resource {
    doc: usize,
    pointer: String,
}

/// A position in a loaded document.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Target {
    pub doc: usize,
    pub pointer: String,
}

impl Documents {
    /// Loads a document and every local document it references.
    pub fn from_file(path: &Path) -> Result<Documents, Error> {
        let source = std::fs::read_to_string(path)
            .map_err(|e| Error::new(format!("cannot read {}: {e}", path.display())))?;
        let value = crate::load(&source)?;
        let path = path
            .canonicalize()
            .map_err(|e| Error::new(format!("cannot read {}: {e}", path.display())))?;
        Documents::new(value, file_url(&path)?)
    }

    /// Like [`Documents::from_file`] for a document already in memory;
    /// relative references are resolved against the current directory.
    pub fn from_value(value: Value) -> Result<Documents, Error> {
        let cwd = current_dir()
            .map_err(|e| Error::new(format!("cannot resolve the current directory: {e}")))?;
        Documents::new(value, file_url(&cwd.join("-"))?)
    }

    fn new(root: Value, retrieval: Url) -> Result<Documents, Error> {
        let mut docs = Documents {
            docs: Vec::new(),
            resources: HashMap::new(),
            anchors: HashMap::new(),
            failures: HashMap::new(),
        };
        docs.add(root, retrieval);
        // Documents are appended while loading, so iterate by index.
        let mut i = 0;
        let mut mapped = 0;
        loop {
            while i < docs.docs.len() {
                docs.scan(i);
                i += 1;
            }
            // A later document's `$self` can map a URI that failed earlier, and
            // OAS 3.2 §4.1.2.1 forbids giving up before every document is parsed:
            // retry the failures whenever another `$self` has been loaded.
            let mapping = docs.docs.iter().filter(|d| *d.base != d.retrieval).count();
            if mapping == mapped {
                return Ok(docs);
            }
            mapped = mapping;
            let mut failed: Vec<String> = docs.failures.keys().cloned().collect();
            failed.sort();
            for key in failed {
                if let Ok(url) = Url::parse(&key)
                    && !docs.resources.contains_key(&key)
                {
                    docs.failures.remove(&key);
                    docs.fetch(&url);
                }
            }
        }
    }

    /// Registers the `$id`s and `$anchor`s of document `i` and loads the
    /// documents it references.
    fn scan(&mut self, i: usize) {
        let doc = &self.docs[i];
        let mut found = Found::default();
        walk(&doc.value, &doc.base, &mut Vec::new(), &mut found);
        for (pointer, base) in found.ids {
            self.resources.insert(
                base.to_string(),
                Resource {
                    doc: i,
                    pointer: pointer.clone(),
                },
            );
            self.docs[i].ids.push((pointer, base));
        }
        for (resource, anchor, pointer) in found.anchors {
            self.anchors.insert((resource, anchor), pointer);
        }
        for target in found.refs {
            let key = target.to_string();
            if !self.resources.contains_key(&key) && !self.failures.contains_key(&key) {
                self.fetch(&target);
            }
        }
    }

    fn add(&mut self, value: Value, retrieval: Url) -> usize {
        let index = self.docs.len();
        let base = value
            .get("$self")
            .and_then(Value::as_str)
            .and_then(|s| retrieval.join(s).ok())
            .map(without_fragment)
            .unwrap_or_else(|| retrieval.clone());
        let resource = Resource {
            doc: index,
            pointer: String::new(),
        };
        self.resources
            .insert(retrieval.to_string(), resource.clone());
        self.resources.insert(base.to_string(), resource);
        self.docs.push(Document {
            retrieval,
            base: Rc::new(base),
            value,
            ids: Vec::new(),
        });
        index
    }

    /// Loads the document identified by `target`, recording a failure instead of
    /// returning it: a `$ref`-like string inside an example must not break loading.
    fn fetch(&mut self, target: &Url) {
        let key = target.to_string();
        let Some(file) = self.to_file(target) else {
            let reason = format!(
                "remote reference `{target}` is not supported; only local files are loaded"
            );
            self.failures.insert(key, reason);
            return;
        };
        let loaded = file_path(&file)
            .ok_or_else(|| format!("`{file}` is not a local file"))
            .and_then(|path| {
                let canonical = path
                    .canonicalize()
                    .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
                let url = file_url(&canonical).map_err(|e| e.to_string())?;
                if let Some(r) = self.resources.get(url.as_str()) {
                    return Ok(Err(r.clone()));
                }
                let source = std::fs::read_to_string(&canonical)
                    .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
                let value = crate::load(&source).map_err(|e| format!("{}: {e}", path.display()))?;
                Ok(Ok((value, url)))
            });
        match loaded {
            Ok(Ok((value, url))) => {
                let index = self.add(value, url);
                self.resources.insert(
                    key,
                    Resource {
                        doc: index,
                        pointer: String::new(),
                    },
                );
            }
            // Already loaded under another URI.
            Ok(Err(resource)) => {
                self.resources.insert(key, resource);
            }
            Err(mut reason) => {
                if file != *target {
                    reason.push_str(&format!(
                        " (mapped from `{target}`; a document identified only by `$self` or `$id` must also be referenced by its file path)"
                    ));
                }
                self.failures.insert(key, reason);
            }
        }
    }

    /// The local file for a URI: `file:` URIs as-is; others (and `file:` URIs
    /// from a relative `$self`) through the closest document whose `$self`
    /// directory contains them.
    fn to_file(&self, target: &Url) -> Option<Url> {
        let mut best: Option<(usize, &Document)> = None;
        for doc in &self.docs {
            if *doc.base == doc.retrieval {
                continue;
            }
            let Ok(dir) = doc.base.join(".") else {
                continue;
            };
            if target.as_str().starts_with(dir.as_str())
                && best.is_none_or(|(len, _)| dir.as_str().len() > len)
            {
                best = Some((dir.as_str().len(), doc));
            }
        }
        if let Some((len, doc)) = best {
            return doc
                .retrieval
                .join(".")
                .ok()?
                .join(&target.as_str()[len..])
                .ok();
        }
        (target.scheme() == "file").then(|| target.clone())
    }

    pub(crate) fn root(&self) -> &Value {
        &self.docs[0].value
    }

    pub(crate) fn root_base(&self) -> Rc<Url> {
        self.docs[0].base.clone()
    }

    pub(crate) fn value(&self, loc: &Target) -> Option<&Value> {
        self.docs[loc.doc].value.pointer(&loc.pointer)
    }

    /// Resolves a URI reference against `base`, returning the target and the
    /// base URI in effect around it (the target's own `$id` is not applied).
    pub(crate) fn resolve(&self, base: &Url, reference: &str) -> Result<(Target, Rc<Url>), Error> {
        let url = base
            .join(reference)
            .map_err(|e| Error::new(format!("invalid $ref `{reference}`: {e}")))?;
        let fragment = percent_decode(url.fragment().unwrap_or(""));
        let key = without_fragment(url).to_string();
        let resource = self
            .resources
            .get(&key)
            .ok_or_else(|| match self.failures.get(&key) {
                Some(reason) => Error::new(format!("cannot resolve $ref `{reference}`: {reason}")),
                None => Error::new(format!("unresolved $ref `{reference}`")),
            })?;
        let pointer = if fragment.is_empty() || fragment.starts_with('/') {
            format!("{}{fragment}", resource.pointer)
        } else {
            self.anchors
                .get(&(key, fragment.clone()))
                .cloned()
                .ok_or_else(|| {
                    Error::new(format!(
                        "unresolved $ref `{reference}`: no `$anchor: {fragment}`"
                    ))
                })?
        };
        let loc = Target {
            doc: resource.doc,
            pointer,
        };
        if self.value(&loc).is_none() {
            return Err(Error::new(format!("unresolved $ref `{reference}`")));
        }
        let doc = &self.docs[loc.doc];
        let base = doc
            .ids
            .iter()
            .filter(|(p, _)| loc.pointer.starts_with(&format!("{p}/")))
            .max_by_key(|(p, _)| p.len())
            .map(|(_, b)| b.clone())
            .unwrap_or_else(|| doc.base.clone());
        Ok((loc, base))
    }

    /// A stable, machine-independent name for a location: `#/pointer` in the
    /// entry document, `relative/path.yaml#/pointer` in others.
    pub(crate) fn display(&self, loc: &Target) -> String {
        if loc.doc == 0 {
            return format!("#{}", loc.pointer);
        }
        let root = &self.docs[0].retrieval;
        let target = &self.docs[loc.doc].retrieval;
        let path = root
            .make_relative(target)
            .unwrap_or_else(|| target.to_string());
        format!("{path}#{}", loc.pointer)
    }
}

#[derive(Default)]
struct Found {
    /// Pointer and base URI of each `$id`.
    ids: Vec<(String, Rc<Url>)>,
    /// Resource URI, anchor name and pointer of each `$anchor`.
    anchors: Vec<(String, String, String)>,
    /// Documents referenced by `$ref` and discriminator mappings.
    refs: Vec<Url>,
}

/// A JSON pointer segment; pointers are only built for `$id` and `$anchor`.
enum Segment<'v> {
    Key(&'v str),
    Index(usize),
}

fn pointer(path: &[Segment]) -> String {
    path.iter()
        .map(|s| match s {
            Segment::Key(k) => format!("/{}", k.replace('~', "~0").replace('/', "~1")),
            Segment::Index(i) => format!("/{i}"),
        })
        .collect()
}

fn walk<'v>(value: &'v Value, base: &Rc<Url>, path: &mut Vec<Segment<'v>>, found: &mut Found) {
    match value {
        Value::Object(map) => {
            let mut base = base.clone();
            if let Some(id) = map.get("$id").and_then(Value::as_str)
                && let Ok(url) = base.join(id)
            {
                base = Rc::new(without_fragment(url));
                found.ids.push((pointer(path), base.clone()));
            }
            if let Some(anchor) = map.get("$anchor").and_then(Value::as_str) {
                found
                    .anchors
                    .push((base.to_string(), anchor.to_string(), pointer(path)));
            }
            let mut refs: Vec<&str> = map
                .get("$ref")
                .and_then(Value::as_str)
                .into_iter()
                .collect();
            if let Some(mapping) = map
                .get("discriminator")
                .and_then(|d| d.get("mapping"))
                .and_then(Value::as_object)
            {
                refs.extend(
                    mapping
                        .values()
                        .filter_map(Value::as_str)
                        .filter(|s| s.contains(['/', '#'])),
                );
            }
            // Fragment-only references stay within an already loaded document.
            for r in refs.into_iter().filter(|r| !r.starts_with('#')) {
                if let Ok(url) = base.join(r) {
                    found.refs.push(without_fragment(url));
                }
            }
            for (k, v) in map {
                path.push(Segment::Key(k));
                walk(v, &base, path, found);
                path.pop();
            }
        }
        Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                path.push(Segment::Index(i));
                walk(v, base, path, found);
                path.pop();
            }
        }
        _ => {}
    }
}

#[cfg(any(unix, windows))]
fn current_dir() -> std::io::Result<PathBuf> {
    std::env::current_dir()?.canonicalize()
}

#[cfg(any(unix, windows))]
fn file_url(path: &Path) -> Result<Url, Error> {
    Url::from_file_path(path)
        .map_err(|()| Error::new(format!("cannot use {} as a URI", path.display())))
}

#[cfg(any(unix, windows))]
fn file_path(url: &Url) -> Option<PathBuf> {
    url.to_file_path().ok()
}

// WebAssembly in a browser, where the documentation site's playground runs the
// generator, has no file system: a document read from text is at `file:///-`,
// and reading any other file fails.

#[cfg(not(any(unix, windows)))]
fn current_dir() -> std::io::Result<PathBuf> {
    Ok(PathBuf::from("/"))
}

#[cfg(not(any(unix, windows)))]
fn file_url(path: &Path) -> Result<Url, Error> {
    Url::parse("file:///")
        .and_then(|root| root.join(&path.to_string_lossy()))
        .map_err(|_| Error::new(format!("cannot use {} as a URI", path.display())))
}

#[cfg(not(any(unix, windows)))]
fn file_path(url: &Url) -> Option<PathBuf> {
    Some(PathBuf::from(percent_decode(url.path())))
}

fn without_fragment(mut url: Url) -> Url {
    url.set_fragment(None);
    url
}

pub(crate) fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = s.get(i + 1..i + 3)
            && let Ok(b) = u8::from_str_radix(hex, 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decodes_pointers() {
        assert_eq!(percent_decode("/paths/~1a%7Bb%7D"), "/paths/~1a{b}");
    }

    #[test]
    fn resolves_ids_and_anchors() {
        let docs = Documents::from_value(json!({
            "$self": "https://example.com/api/openapi",
            "components": {"schemas": {"A": {
                "$id": "schemas/a",
                "$defs": {"b": {"$anchor": "bee", "type": "string"}}
            }}}
        }))
        .unwrap();
        let base = docs.root_base();
        assert_eq!(base.as_str(), "https://example.com/api/openapi");
        let (loc, _) = docs.resolve(&base, "schemas/a#bee").unwrap();
        assert_eq!(loc.pointer, "/components/schemas/A/$defs/b");
        let (loc, inner) = docs.resolve(&base, "schemas/a#/$defs/b").unwrap();
        assert_eq!(loc.pointer, "/components/schemas/A/$defs/b");
        assert_eq!(inner.as_str(), "https://example.com/api/schemas/a");
    }

    #[test]
    fn resolves_uris_mapped_by_a_later_documents_self() {
        // `lib/index.yaml` maps `https://api.example.com/schemas/` onto `lib/`, but
        // only once it is loaded; the result must not depend on reference order.
        let dir = std::env::temp_dir().join(format!("oasis-gen-later-self-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir); // left behind by a failed run
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        std::fs::write(
            dir.join("lib/index.yaml"),
            "$self: https://api.example.com/schemas/index.yaml\n",
        )
        .unwrap();
        std::fs::write(dir.join("lib/pet.yaml"), "type: object\n").unwrap();
        let pet = "https://api.example.com/schemas/pet.yaml";
        for refs in [[pet, "lib/index.yaml"], ["lib/index.yaml", pet]] {
            let entry = dir.join("openapi.yaml");
            let doc = json!({"a": {"$ref": refs[0]}, "b": {"$ref": refs[1]}});
            std::fs::write(&entry, doc.to_string()).unwrap();
            let docs = Documents::from_file(&entry).unwrap();
            let result = docs.resolve(&docs.root_base(), pet);
            assert!(result.is_ok(), "{refs:?}: {}", result.err().unwrap());
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn loads_a_file_once_under_its_path_and_its_self_uri() {
        let dir = std::env::temp_dir().join(format!("oasis-gen-two-uris-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir); // left behind by a failed run
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        std::fs::write(
            dir.join("lib/index.yaml"),
            "$self: https://api.example.com/schemas/index.yaml\n",
        )
        .unwrap();
        std::fs::write(dir.join("lib/pet.yaml"), "type: object\n").unwrap();
        let pet = "https://api.example.com/schemas/pet.yaml";
        let entry = dir.join("openapi.yaml");
        let doc = json!({"a": {"$ref": "lib/index.yaml"}, "b": {"$ref": "lib/pet.yaml"}, "c": {"$ref": pet}});
        std::fs::write(&entry, doc.to_string()).unwrap();
        let docs = Documents::from_file(&entry).unwrap();
        let (by_path, _) = docs.resolve(&docs.root_base(), "lib/pet.yaml").unwrap();
        let (by_uri, _) = docs.resolve(&docs.root_base(), pet).unwrap();
        assert_eq!(by_path, by_uri);
        assert_eq!(docs.docs.len(), 3); // the entry, lib/index.yaml and lib/pet.yaml
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reports_remote_references() {
        let docs =
            Documents::from_value(json!({"x": {"$ref": "https://elsewhere.example/a.yaml"}}))
                .unwrap();
        let err = docs
            .resolve(&docs.root_base(), "https://elsewhere.example/a.yaml")
            .unwrap_err();
        assert!(err.to_string().contains("only local files"), "{err}");
    }
}
