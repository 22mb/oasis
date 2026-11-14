//! Loading OpenAPI documents and resolving URI references between them.
//!
//! Base URIs follow OpenAPI 3.2 and JSON Schema 2020-12: a document's `$self`
//! (resolved against its retrieval URI) and a schema's `$id` override the
//! retrieval URI. Documents are identified by `$self` and by retrieval URI;
//! schema resources by `$id`. Only local files are loaded. A URI under a
//! document's `$self` directory is mapped to the same path relative to that
//! document's file (`$id` directories are not mapped).
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
    /// The document that generation starts from.
    entry: Document,
    /// Documents loaded through references, in the order they were found.
    referenced: Vec<Document>,
    /// Absolute URI without fragment -> resource it identifies.
    resources: HashMap<String, Target>,
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
    /// The name shown in errors, and the source text for finding error
    /// positions.
    file: Option<(String, String)>,
}

/// A loaded document: the entry document, or one loaded through a reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DocId {
    Entry,
    Referenced(usize),
}

/// A position in a loaded document.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Target {
    pub doc: DocId,
    pub pointer: String,
}

impl Document {
    fn new(value: Value, retrieval: Url, file: Option<(String, String)>) -> Document {
        let base = value
            .get("$self")
            .and_then(Value::as_str)
            .and_then(|s| retrieval.join(s).ok())
            .map(without_fragment)
            .unwrap_or_else(|| retrieval.clone());
        Document {
            retrieval,
            base: Rc::new(base),
            value,
            ids: Vec::new(),
            file,
        }
    }
}

impl Documents {
    /// Loads a document and every local document it references.
    pub fn from_file(path: &Path) -> Result<Documents, Error> {
        let source = std::fs::read_to_string(path)
            .map_err(|e| Error::new(format!("cannot read {}: {e}", path.display())))?;
        let name = path.display().to_string();
        let value = crate::load(&source).map_err(|e| e.in_file(&name))?;
        let canonical = path
            .canonicalize()
            .map_err(|e| Error::new(format!("cannot read {name}: {e}")))?;
        Documents::new(value, file_url(&canonical)?, Some((name, source)))
    }

    /// Like [`Documents::from_file`] for document text, which errors call
    /// `<input>`; relative references are resolved against the current
    /// directory.
    pub fn from_text(source: &str) -> Result<Documents, Error> {
        let name = "<input>";
        let value = crate::load(source).map_err(|e| e.in_file(name))?;
        Documents::new(
            value,
            current_dir_url()?,
            Some((name.into(), source.into())),
        )
    }

    /// Attaches a location to `e`: the file name, and the line and column of
    /// the value that caused it. Errors not tied to a particular value are
    /// placed in the entry document.
    pub(crate) fn locate(&self, e: Error) -> Error {
        if e.is_placed() {
            return e;
        }
        let found = e.value_address().and_then(|address| {
            self.all()
                .find_map(|(id, d)| find(&d.value, address).map(|pointer| (id, pointer)))
        });
        let doc = found.as_ref().map_or(&self.entry, |(id, _)| self.doc(*id));
        let Some((name, source)) = &doc.file else {
            return e;
        };
        let e = e.in_file(name);
        match found.and_then(|(_, pointer)| position(source, &pointer)) {
            Some((line, column)) => e.at(line, column),
            None => e,
        }
    }

    /// Like [`Documents::from_file`] for a document already in memory, whose
    /// errors have no file name or position; relative references are resolved
    /// against the current directory.
    pub fn from_value(value: Value) -> Result<Documents, Error> {
        Documents::new(value, current_dir_url()?, None)
    }

    fn new(
        root: Value,
        retrieval: Url,
        file: Option<(String, String)>,
    ) -> Result<Documents, Error> {
        let mut docs = Documents {
            entry: Document::new(root, retrieval, file),
            referenced: Vec::new(),
            resources: HashMap::new(),
            anchors: HashMap::new(),
            failures: HashMap::new(),
        };
        docs.register(DocId::Entry);
        docs.scan(DocId::Entry);
        // Documents are appended while loading, so iterate by index.
        let mut i = 0;
        let mut mapped = 0;
        loop {
            while i < docs.referenced.len() {
                docs.scan(DocId::Referenced(i));
                i += 1;
            }
            // A later document's `$self` can map a URI that failed earlier, and
            // OAS 3.2 §4.1.2.1 forbids giving up before every document is
            // parsed, so retry failed URIs whenever a new `$self` is loaded.
            let mapping = docs.all().filter(|(_, d)| *d.base != d.retrieval).count();
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

    /// Registers the `$id`s and `$anchor`s of document `id` and loads the
    /// documents it references.
    fn scan(&mut self, id: DocId) {
        let doc = self.doc(id);
        let mut found = Found::default();
        walk(&doc.value, &doc.base, &mut Vec::new(), &mut found);
        for (pointer, base) in found.ids {
            self.resources.insert(
                base.to_string(),
                Target {
                    doc: id,
                    pointer: pointer.clone(),
                },
            );
            self.doc_mut(id).ids.push((pointer, base));
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

    /// Adds a document loaded through a reference.
    fn add(&mut self, value: Value, retrieval: Url, file: Option<(String, String)>) -> Target {
        self.referenced.push(Document::new(value, retrieval, file));
        self.register(DocId::Referenced(self.referenced.len() - 1))
    }

    /// Makes document `id` reachable by its retrieval URI and its `$self`.
    fn register(&mut self, id: DocId) -> Target {
        let doc = self.doc(id);
        let uris = [doc.retrieval.to_string(), doc.base.to_string()];
        let resource = Target {
            doc: id,
            pointer: String::new(),
        };
        for uri in uris {
            self.resources.insert(uri, resource.clone());
        }
        resource
    }

    fn doc(&self, id: DocId) -> &Document {
        match id {
            DocId::Entry => &self.entry,
            DocId::Referenced(i) => &self.referenced[i],
        }
    }

    fn doc_mut(&mut self, id: DocId) -> &mut Document {
        match id {
            DocId::Entry => &mut self.entry,
            DocId::Referenced(i) => &mut self.referenced[i],
        }
    }

    /// Every loaded document, the entry document first.
    fn all(&self) -> impl Iterator<Item = (DocId, &Document)> {
        let referenced = self.referenced.iter().enumerate();
        std::iter::once((DocId::Entry, &self.entry))
            .chain(referenced.map(|(i, d)| (DocId::Referenced(i), d)))
    }

    /// Loads the document identified by `target`, recording failures instead of
    /// returning them because a `$ref`-like string in an example must not break
    /// loading.
    fn fetch(&mut self, target: &Url) {
        let key = target.to_string();
        match self.load(target) {
            Ok(resource) => {
                self.resources.insert(key, resource);
            }
            Err(reason) => {
                self.failures.insert(key, reason);
            }
        }
    }

    fn load(&mut self, target: &Url) -> Result<Target, String> {
        let file = self.to_file(target).ok_or_else(|| {
            format!("remote reference `{target}` is not supported; only local files are loaded")
        })?;
        self.load_file(&file).map_err(|reason| {
            // A file that does not exist may be a document identified by `$self`
            // that was never loaded under its file name.
            let missing = file_path(&file).is_some_and(|p| !p.exists());
            if file == *target || !missing {
                return reason;
            }
            format!(
                "{reason} (mapped from `{target}`; a document identified only by `$self` or `$id` must also be referenced by its file path)"
            )
        })
    }

    fn load_file(&mut self, file: &Url) -> Result<Target, String> {
        let path = file_path(file).ok_or_else(|| format!("`{file}` is not a local file"))?;
        let unreadable = |path: &Path, e| format!("cannot read {}: {e}", display_path(path));
        let canonical = path.canonicalize().map_err(|e| unreadable(&path, e))?;
        let url = file_url(&canonical).map_err(|e| e.to_string())?;
        if let Some(r) = self.resources.get(url.as_str()) {
            return Ok(r.clone()); // already loaded under another URI
        }
        // Keep it canonical to match the current directory it is relative to.
        let source = std::fs::read_to_string(&canonical).map_err(|e| unreadable(&canonical, e))?;
        let name = display_path(&canonical);
        let value = crate::load(&source).map_err(|e| e.in_file(&name).to_string())?;
        Ok(self.add(value, url, Some((name, source))))
    }

    /// Maps a URI to a local `file:` URL. URIs inside a document's `$self`
    /// directory (including `file:` URIs from a relative `$self`) are mapped
    /// through the closest matching document; other `file:` URIs are used
    /// as-is.
    fn to_file(&self, target: &Url) -> Option<Url> {
        let mut best: Option<(usize, &Document)> = None;
        for (_, doc) in self.all() {
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
        &self.entry.value
    }

    pub(crate) fn root_base(&self) -> Rc<Url> {
        self.entry.base.clone()
    }

    /// Resolves a URI reference against `base`, returning the target, its value
    /// and the base URI in effect around it (the target's own `$id` is not applied).
    pub(crate) fn resolve(
        &self,
        base: &Url,
        reference: &str,
    ) -> Result<(Target, &Value, Rc<Url>), Error> {
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
        let doc = self.doc(resource.doc);
        let value = doc
            .value
            .pointer(&pointer)
            .ok_or_else(|| Error::new(format!("unresolved $ref `{reference}`")))?;
        let loc = Target {
            doc: resource.doc,
            pointer,
        };
        let base = doc
            .ids
            .iter()
            .filter(|(p, _)| loc.pointer.starts_with(&format!("{p}/")))
            .max_by_key(|(p, _)| p.len())
            .map(|(_, b)| b.clone())
            .unwrap_or_else(|| doc.base.clone());
        Ok((loc, value, base))
    }

    /// A stable, machine-independent name for a location: `#/pointer` in the
    /// entry document, `relative/path.yaml#/pointer` in others.
    pub(crate) fn display(&self, loc: &Target) -> String {
        if loc.doc == DocId::Entry {
            return format!("#{}", loc.pointer);
        }
        let root = &self.entry.retrieval;
        let target = &self.doc(loc.doc).retrieval;
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

/// Returns `path` relative to the current directory when inside it, as a user
/// would type it.
fn display_path(path: &Path) -> String {
    current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok().map(Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf())
        .display()
        .to_string()
}

/// The JSON pointer of the value at `address` in `v`.
fn find(v: &Value, address: usize) -> Option<String> {
    if std::ptr::eq(v, address as *const Value) {
        return Some(String::new());
    }
    let escape = |k: &str| k.replace('~', "~0").replace('/', "~1");
    match v {
        Value::Object(map) => map
            .iter()
            .find_map(|(k, v)| find(v, address).map(|p| format!("/{}{p}", escape(k)))),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .find_map(|(i, v)| find(v, address).map(|p| format!("/{i}{p}"))),
        _ => None,
    }
}

/// Line and column of the value at JSON pointer `pointer` in document text
/// `source`. JSON is read as YAML, of which it is a subset, since only the YAML
/// parser reports positions. Only called for an error, so its cost does not matter.
fn position(source: &str, pointer: &str) -> Option<(usize, usize)> {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let mut options = crate::yaml_options(source);
    // Allow the same depth as serde_json, since an error can be at a value
    // deeper than YAML allows.
    options
        .budget
        .get_or_insert_with(Default::default)
        .max_depth = 128;
    let mut node: serde_saphyr::Spanned<Spans> =
        serde_saphyr::from_str_with_options(source, options).ok()?;
    for segment in pointer.split('/').skip(1) {
        let key = segment.replace("~1", "/").replace("~0", "~");
        node = match node.value {
            Spans::Map(entries) => entries.into_iter().find(|(k, _)| *k == key)?.1,
            Spans::Seq(items) => items.into_iter().nth(key.parse().ok()?)?,
            Spans::Scalar => return None,
        };
    }
    let at = node.referenced;
    Some((at.line() as usize, at.column() as usize))
}

/// A document's structure, with the position of every value.
enum Spans {
    Scalar,
    Seq(Vec<serde_saphyr::Spanned<Spans>>),
    Map(Vec<(String, serde_saphyr::Spanned<Spans>)>),
}

impl<'de> serde::Deserialize<'de> for Spans {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Spans, D::Error> {
        use serde::de::{MapAccess, SeqAccess, Visitor};
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Spans;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("any value")
            }
            fn visit_bool<E>(self, _: bool) -> Result<Spans, E> {
                Ok(Spans::Scalar)
            }
            fn visit_i64<E>(self, _: i64) -> Result<Spans, E> {
                Ok(Spans::Scalar)
            }
            fn visit_u64<E>(self, _: u64) -> Result<Spans, E> {
                Ok(Spans::Scalar)
            }
            fn visit_f64<E>(self, _: f64) -> Result<Spans, E> {
                Ok(Spans::Scalar)
            }
            fn visit_str<E>(self, _: &str) -> Result<Spans, E> {
                Ok(Spans::Scalar)
            }
            fn visit_unit<E>(self) -> Result<Spans, E> {
                Ok(Spans::Scalar)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<Spans, A::Error> {
                let mut items = Vec::new();
                while let Some(item) = s.next_element()? {
                    items.push(item);
                }
                Ok(Spans::Seq(items))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<Spans, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = m.next_entry()? {
                    entries.push(entry);
                }
                Ok(Spans::Map(entries))
            }
        }
        d.deserialize_any(V)
    }
}

/// A `file:` URL in the current directory, as the retrieval URI of a
/// document that was not read from a file.
fn current_dir_url() -> Result<Url, Error> {
    let cwd = current_dir()
        .map_err(|e| Error::new(format!("cannot resolve the current directory: {e}")))?;
    file_url(&cwd.join("-"))
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
// generator, has no file system. A document read from text is placed at
// `file:///-`, and reading any other file fails.

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
        let (loc, _, _) = docs.resolve(&base, "schemas/a#bee").unwrap();
        assert_eq!(loc.pointer, "/components/schemas/A/$defs/b");
        let (loc, _, inner) = docs.resolve(&base, "schemas/a#/$defs/b").unwrap();
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
        let (by_path, _, _) = docs.resolve(&docs.root_base(), "lib/pet.yaml").unwrap();
        let (by_uri, _, _) = docs.resolve(&docs.root_base(), pet).unwrap();
        assert_eq!(by_path, by_uri);
        assert_eq!(docs.referenced.len(), 2); // lib/index.yaml and lib/pet.yaml
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn says_which_uri_a_missing_file_was_mapped_from() {
        let dir =
            std::env::temp_dir().join(format!("oasis-gen-missing-self-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir); // left behind by a failed run
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        std::fs::write(
            dir.join("lib/index.yaml"),
            "$self: https://api.example.com/schemas/index.yaml\n",
        )
        .unwrap();
        let pet = "https://api.example.com/schemas/pet.yaml";
        let entry = dir.join("openapi.yaml");
        let doc = json!({"a": {"$ref": "lib/index.yaml"}, "b": {"$ref": pet}});
        std::fs::write(&entry, doc.to_string()).unwrap();
        let docs = Documents::from_file(&entry).unwrap();
        let err = docs
            .resolve(&docs.root_base(), pet)
            .unwrap_err()
            .to_string();
        // The reason before the hint is the operating system's.
        assert!(err.contains(&format!("(mapped from `{pet}`;")), "{err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
