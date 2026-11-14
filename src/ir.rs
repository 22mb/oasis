//! Language-neutral intermediate representation of an OpenAPI 3.2 document.
//!
//! Emitters for target languages consume this instead of the raw document, so
//! `$ref` resolution, version checks and validation live in one place.
//!
//! The IR is public so that other emitters can be built on it, but it is not
//! yet stable: expect breaking changes in 0.x releases.

use serde_json::Value;
use std::collections::{BTreeSet, HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct Api {
    pub title: String,
    pub version: String,
    /// Top-level `servers`. Empty when the document declares none.
    pub servers: Vec<Server>,
    /// Every schema that [`Schema::Ref`] can point to: `components.schemas` in
    /// document order, followed by schemas hoisted from other `$ref` targets.
    /// Every `Ref` key is guaranteed to be present here.
    pub schemas: Vec<NamedSchema>,
    /// In document order.
    pub paths: Vec<PathItem>,
    /// `components.securitySchemes`, plus schemes referenced by URI from
    /// security requirements.
    pub security_schemes: Vec<SecurityScheme>,
    /// Top-level `security`: the default for operations that declare none.
    pub security: Vec<SecurityRequirement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NamedSchema {
    /// The component name, or for hoisted schemas the location of the `$ref`
    /// target: `#/pointer` in the entry document, `relative/path.yaml#/pointer`
    /// in another file. Component names cannot contain `#`, so keys never
    /// collide.
    pub key: String,
    /// `true` for schemas hoisted from `$ref`s outside `components.schemas`
    /// (e.g. `#/components/schemas/Tree/$defs/node`).
    pub hoisted: bool,
    pub meta: Meta,
    pub schema: Schema,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Server {
    /// URL template, possibly with `{variable}` placeholders.
    pub url: String,
    pub variables: Vec<ServerVariable>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerVariable {
    pub name: String,
    pub default: String,
}

impl Server {
    /// The URL with every variable substituted by its default.
    pub fn default_url(&self) -> String {
        self.variables.iter().fold(self.url.clone(), |url, v| {
            url.replace(&format!("{{{}}}", v.name), &v.default)
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathItem {
    /// Path template, e.g. `/pets/{petId}`.
    pub path: String,
    /// Overrides the top-level servers for this path when non-empty.
    pub servers: Vec<Server>,
    /// Fixed methods in spec order (`get` ... `query`), then `additionalOperations`.
    pub operations: Vec<Operation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Operation {
    /// HTTP method exactly as sent on the wire, e.g. `GET`, `QUERY`, `LINK`.
    pub method: String,
    /// Unique across the document when present.
    pub operation_id: Option<String>,
    pub meta: Meta,
    /// Overrides the path and top-level servers when non-empty.
    pub servers: Vec<Server>,
    /// Path-level parameters merged with operation-level ones.
    pub params: Vec<Param>,
    /// `in: querystring` parameter (new in 3.2).
    pub querystring: Option<QueryString>,
    pub body: Option<Body>,
    /// In document order.
    pub responses: Vec<Response>,
    /// `None` inherits [`Api::security`]; `Some(vec![])` requires no security.
    pub security: Option<Vec<SecurityRequirement>>,
    /// The Operation Object.
    pub origin: Origin,
}

/// Where in the documents a node was read from, so that an emitter's errors
/// can point at it. Opaque, and meaningful only with the documents the [`Api`]
/// was parsed from. It is not part of the node's value: all origins compare
/// equal, and `Origin::default()`, for a node built by hand, points nowhere.
#[derive(Clone, Copy, Default)]
pub struct Origin(usize);

impl Origin {
    pub(crate) fn of(v: &Value) -> Origin {
        Origin(v as *const Value as usize)
    }

    /// The address of the document value, or `None` for the default.
    pub(crate) fn address(self) -> Option<usize> {
        (self.0 != 0).then_some(self.0)
    }
}

impl PartialEq for Origin {
    fn eq(&self, _: &Origin) -> bool {
        true
    }
}

impl std::fmt::Debug for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Origin")
    }
}

/// One alternative of a `security` list: every scheme in it applies together.
/// Alternatives are OR-ed; an empty requirement allows anonymous access.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SecurityRequirement {
    /// Scheme key in [`Api::security_schemes`] and the required scopes or roles.
    pub schemes: Vec<(String, Vec<String>)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SecurityScheme {
    /// The component name, or for schemes referenced by URI the location of the
    /// target (always containing `#`).
    pub key: String,
    pub meta: Meta,
    pub kind: SecuritySchemeKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SecuritySchemeKind {
    ApiKey {
        name: String,
        location: ApiKeyLocation,
    },
    Http {
        /// Lowercased, e.g. `bearer` or `basic`.
        scheme: String,
    },
    MutualTls,
    OAuth2,
    OpenIdConnect {
        url: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiKeyLocation {
    Query,
    Header,
    Cookie,
}

/// Documentation and annotations shared by schemas, properties, operations,
/// parameters and responses. Fields that do not apply stay at their defaults.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Meta {
    pub title: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub deprecated: bool,
    /// Schema `readOnly`: should not be sent in requests.
    pub read_only: bool,
    /// Schema `writeOnly`: should not be returned in responses.
    pub write_only: bool,
    /// Schema `default`.
    pub default: Option<Value>,
    /// `x-*` specification extensions, in document order.
    pub extensions: Vec<(String, Value)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    Path,
    Query,
    Header,
    Cookie,
}

impl Location {
    pub fn as_str(self) -> &'static str {
        match self {
            Location::Path => "path",
            Location::Query => "query",
            Location::Header => "header",
            Location::Cookie => "cookie",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub location: Location,
    pub required: bool,
    pub meta: Meta,
    pub value: ParamValue,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParamValue {
    /// Described by `schema`, serialized according to `style`.
    Schema {
        style: Style,
        /// Explicit or defaulted (`true` for `form` and `cookie`, else `false`).
        explode: bool,
        /// `allowReserved`; only meaningful for query parameters.
        allow_reserved: bool,
        schema: Schema,
    },
    /// Described by `content` with exactly one media type.
    Content(Content),
}

/// Parameter styles oasis-gen supports. Others are rejected while parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// `form`: query and cookie (default for both).
    Form,
    /// `simple`: path and header (default for both).
    Simple,
    /// `deepObject`: query.
    DeepObject,
    /// `cookie`: cookie, without percent-encoding.
    Cookie,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueryString {
    pub required: bool,
    pub meta: Meta,
    pub content: Content,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Body {
    pub required: bool,
    pub meta: Meta,
    pub content: Vec<Content>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    pub status: Status,
    pub meta: Meta,
    pub content: Vec<Content>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// A status code, `100`..=`599`.
    Code(u16),
    /// A range such as `2XX`, stored as its first digit (`1`..=`5`).
    Range(u8),
    Default,
}

impl Status {
    /// The key as written in the document: `200`, `2XX` or `default`.
    pub fn key(self) -> String {
        match self {
            Status::Code(c) => c.to_string(),
            Status::Range(d) => format!("{d}XX"),
            Status::Default => "default".into(),
        }
    }
}

impl Operation {
    /// The statuses each response covers, in ascending order: a single code, or
    /// a range such as `4XX` minus any codes declared in it, in document order.
    /// The last entry covers every remaining status from 200 to 599, paired
    /// with the `default` response if declared (`None` otherwise, as a response
    /// can still have those statuses). Like 1xx statuses, 304 is excluded from
    /// this fallback because a server sends it only for a conditional request
    /// and fetch returns it only when the caller made that request itself, so
    /// it is covered only when an explicit code or `3XX` range includes it.
    pub fn coverage(&self) -> Vec<(Option<&Response>, Vec<u16>)> {
        let mut covered = [false; 600];
        for r in &self.responses {
            match r.status {
                Status::Code(c) => covered[usize::from(c)] = true,
                Status::Range(d) => covered[usize::from(d) * 100..][..100].fill(true),
                Status::Default => {}
            }
        }
        let codes = |r: &Response| match r.status {
            Status::Code(c) => Some(vec![c]),
            Status::Range(d) => {
                let first = u16::from(d) * 100;
                let declared =
                    |s: &u16| self.responses.iter().any(|r| r.status == Status::Code(*s));
                Some((first..first + 100).filter(|s| !declared(s)).collect())
            }
            Status::Default => None,
        };
        let mut out: Vec<_> = self
            .responses
            .iter()
            .filter_map(|r| Some((Some(r), codes(r)?)))
            .collect();
        let rest = (200..600)
            .filter(|&s| s != 304 && !covered[usize::from(s)])
            .collect();
        let default = self.responses.iter().find(|r| r.status == Status::Default);
        out.push((default, rest));
        out
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Content {
    pub media_type: String,
    pub schema: Option<Schema>,
    /// Schema of each item of a sequential media type (new in 3.2).
    pub item_schema: Option<Schema>,
    /// `encoding`: how each named property is sent, in document order. Only
    /// read for `application/x-www-form-urlencoded` and `multipart/form-data`;
    /// properties without an entry use the defaults for their value.
    pub encoding: Vec<Encoding>,
}

/// How one property of a form body is sent (Encoding Object).
#[derive(Debug, Clone, PartialEq)]
pub struct Encoding {
    /// The property.
    pub name: String,
    /// `None` when the object gives neither `contentType` nor `style`,
    /// `explode` or `allowReserved`.
    pub serialization: Option<Serialization>,
    /// `headers` of a `multipart/form-data` part, without `Content-Type`,
    /// which the specification ignores. Empty for other media types.
    pub headers: Vec<PartHeader>,
    /// The Encoding Object.
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartHeader {
    pub name: String,
    pub required: bool,
}

/// What an Encoding Object says about the property's value.
#[derive(Debug, Clone, PartialEq)]
pub enum Serialization {
    /// As `contentType`.
    ContentType(String),
    /// Like a query parameter, when `style`, `explode` or `allowReserved` is
    /// given; `contentType` is then ignored.
    Style {
        style: Style,
        explode: bool,
        allow_reserved: bool,
    },
}

/// How a media type's payload is represented, independent of target language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Json,
    FormUrlEncoded,
    Multipart,
    Text,
    /// `text/event-stream`
    EventStream,
    /// `application/jsonl`, `application/x-ndjson`
    JsonLines,
    /// `application/json-seq` and `+json-seq`
    JsonSeq,
    /// A range such as `*/*` or `application/*` that may be JSON or binary;
    /// the actual response `Content-Type` decides.
    Unknown,
    /// Anything else: raw bytes.
    Binary,
}

/// The media type without parameters, lowercased: `application/json; charset=utf-8` -> `application/json`.
pub fn essence(media_type: &str) -> String {
    media_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

impl MediaKind {
    pub fn of(media_type: &str) -> MediaKind {
        let mt = essence(media_type);
        match mt.as_str() {
            "application/json" => MediaKind::Json,
            "application/x-www-form-urlencoded" => MediaKind::FormUrlEncoded,
            "text/event-stream" => MediaKind::EventStream,
            "application/jsonl" | "application/x-ndjson" => MediaKind::JsonLines,
            "application/json-seq" => MediaKind::JsonSeq,
            "*/*" | "application/*" => MediaKind::Unknown,
            _ if mt.ends_with("+json") => MediaKind::Json,
            _ if mt.ends_with("+json-seq") => MediaKind::JsonSeq,
            _ if mt.starts_with("multipart/") => MediaKind::Multipart,
            _ if mt.starts_with("text/") => MediaKind::Text,
            _ => MediaKind::Binary,
        }
    }

    pub fn is_sequential(self) -> bool {
        matches!(
            self,
            MediaKind::EventStream | MediaKind::JsonLines | MediaKind::JsonSeq
        )
    }
}

impl Content {
    pub fn kind(&self) -> MediaKind {
        MediaKind::of(&self.media_type)
    }

    /// Schema of one item of a sequential media type: `itemSchema`, or the
    /// `items` of an array `schema` describing the whole sequence.
    pub fn sequence_item(&self) -> Option<&Schema> {
        if let Some(s) = &self.item_schema {
            return Some(s);
        }
        match &self.schema {
            Some(Schema::Array {
                items: Some(items), ..
            }) => Some(items),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Schema {
    /// `true` or `{}`.
    Any,
    /// `false`.
    Never,
    /// Key of an entry in [`Api::schemas`].
    Ref(String),
    Null,
    Boolean,
    Integer {
        format: Option<String>,
    },
    Number {
        format: Option<String>,
    },
    String {
        format: Option<String>,
        content_encoding: Option<String>,
    },
    Const {
        value: Value,
        /// The schema's `type`, if declared.
        base: Option<Box<Schema>>,
    },
    Enum {
        values: Vec<Value>,
        /// The schema's `type`, if declared.
        base: Option<Box<Schema>>,
    },
    Array {
        prefix_items: Vec<Schema>,
        /// `None` means unconstrained; `Some(Never)` closes a tuple.
        items: Option<Box<Schema>>,
        /// `minItems`, or 0.
        min_items: u64,
    },
    Object {
        /// Includes names listed only in `required` (with schema `Any`).
        properties: Vec<Property>,
        /// `patternProperties`: the schema of the properties whose names match
        /// each regular expression, in document order.
        pattern_properties: Vec<(String, Schema)>,
        /// `None` when `additionalProperties` is absent; `Some(Any)` when it
        /// is explicitly `true`. Both allow any extra property. It applies to
        /// the properties that neither `properties` nor `patternProperties` cover.
        additional: Option<Box<Schema>>,
    },
    Union {
        kind: UnionKind,
        variants: Vec<Schema>,
        discriminator: Option<Discriminator>,
    },
    /// `allOf`, or `$ref` with sibling keywords.
    Intersection(Vec<Schema>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnionKind {
    /// `oneOf`: exactly one variant matches.
    OneOf,
    /// `anyOf`: one or more variants match.
    AnyOf,
    /// A `type` array such as `["string", "null"]`.
    Types,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Discriminator {
    pub property_name: String,
    /// Discriminating value -> schema key in [`Api::schemas`]: the explicit
    /// `mapping`, then the implicit one (component name -> component) for
    /// each `Ref` variant that names a component, unless `mapping` has that
    /// value or the variant is the target of [`Self::default_mapping`].
    pub mapping: Vec<(String, String)>,
    /// `defaultMapping`: the schema key that takes every value not in
    /// [`Self::mapping`].
    pub default_mapping: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub name: String,
    pub required: bool,
    pub meta: Meta,
    pub schema: Schema,
}

impl Body {
    /// The media type the body is sent as when the call names none: the only
    /// one declared, or else the first `application/json` one.
    pub fn default_media_type(&self) -> Option<&str> {
        let declared: Vec<&str> = self
            .content
            .iter()
            .map(|c| c.media_type.as_str())
            .filter(|m| !essence(m).contains('*'))
            .collect();
        match declared.as_slice() {
            [only] if self.content.len() == 1 => Some(only),
            _ => declared
                .into_iter()
                .find(|m| essence(m) == "application/json"),
        }
    }
}

impl Schema {
    /// Visits the schema tree without following references.
    pub fn walk<'s>(&'s self, f: &mut dyn FnMut(&'s Schema)) {
        f(self);
        match self {
            Schema::Array {
                prefix_items,
                items,
                ..
            } => {
                prefix_items.iter().for_each(|c| c.walk(f));
                if let Some(c) = items {
                    c.walk(f);
                }
            }
            Schema::Object {
                properties,
                pattern_properties,
                additional,
            } => {
                properties.iter().for_each(|p| p.schema.walk(f));
                pattern_properties.iter().for_each(|(_, c)| c.walk(f));
                if let Some(c) = additional {
                    c.walk(f);
                }
            }
            Schema::Union { variants, .. } | Schema::Intersection(variants) => {
                variants.iter().for_each(|c| c.walk(f))
            }
            _ => {}
        }
    }

    /// [`Schema::walk`], with each node mutable before its children are visited.
    pub fn walk_mut(&mut self, f: &mut dyn FnMut(&mut Schema)) {
        f(self);
        match self {
            Schema::Array {
                prefix_items,
                items,
                ..
            } => {
                prefix_items.iter_mut().for_each(|c| c.walk_mut(f));
                if let Some(c) = items {
                    c.walk_mut(f);
                }
            }
            Schema::Object {
                properties,
                pattern_properties,
                additional,
            } => {
                properties.iter_mut().for_each(|p| p.schema.walk_mut(f));
                pattern_properties
                    .iter_mut()
                    .for_each(|(_, c)| c.walk_mut(f));
                if let Some(c) = additional {
                    c.walk_mut(f);
                }
            }
            Schema::Union { variants, .. } | Schema::Intersection(variants) => {
                variants.iter_mut().for_each(|c| c.walk_mut(f))
            }
            _ => {}
        }
    }
}

/// The named schemas of an API by key, for reading a schema through its
/// references.
pub struct Schemas<'a>(HashMap<&'a str, &'a NamedSchema>);

impl<'a> std::ops::Deref for Schemas<'a> {
    type Target = HashMap<&'a str, &'a NamedSchema>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> Schemas<'a> {
    pub fn new(schemas: &'a [NamedSchema]) -> Schemas<'a> {
        Schemas(schemas.iter().map(|s| (s.key.as_str(), s)).collect())
    }

    /// `s`, with references followed. A cycle of references ends at one of
    /// its references.
    pub fn resolve<'s>(&'s self, mut s: &'s Schema) -> &'s Schema {
        for _ in 0..=self.0.len() {
            let Schema::Ref(k) = s else { break };
            s = &self.0[k.as_str()].schema;
        }
        s
    }

    /// `s` and every schema it reaches through `$ref` and `allOf` alone, in
    /// document order, following each reference once.
    pub fn flatten<'s>(&self, s: &'s Schema) -> impl Iterator<Item = &'s Schema>
    where
        'a: 's,
    {
        let mut stack = vec![s];
        let mut seen = HashSet::new();
        std::iter::from_fn(move || {
            let s = stack.pop()?;
            match s {
                Schema::Ref(k) if seen.insert(k.as_str()) => stack.push(&self.0[k.as_str()].schema),
                Schema::Intersection(parts) => stack.extend(parts.iter().rev()),
                _ => {}
            }
            Some(s)
        })
    }

    /// Whether a value of `s` is `readOnly` and `writeOnly` through `$ref` and
    /// `allOf` alone.
    pub fn access(&self, s: &Schema) -> (bool, bool) {
        self.flatten(s).fold((false, false), |(r, w), s| match s {
            Schema::Ref(k) => {
                let meta = &self.0[k.as_str()].meta;
                (r || meta.read_only, w || meta.write_only)
            }
            _ => (r, w),
        })
    }

    /// The names of the `readOnly` and `writeOnly` properties of `parts`, an
    /// `allOf`, with nested `allOf` members and `$ref` targets flattened.
    pub fn flagged_in(&self, parts: &[Schema]) -> (BTreeSet<String>, BTreeSet<String>) {
        let mut flagged = (BTreeSet::new(), BTreeSet::new());
        for s in parts.iter().flat_map(|p| self.flatten(p)) {
            let Schema::Object { properties, .. } = s else {
                continue;
            };
            for p in properties {
                let (r, w) = self.access(&p.schema);
                if p.meta.read_only || r {
                    flagged.0.insert(p.name.clone());
                }
                if p.meta.write_only || w {
                    flagged.1.insert(p.name.clone());
                }
            }
        }
        flagged
    }

    /// The schema of property `name` of `s`, through references and `allOf`.
    pub fn property(&self, s: &'a Schema, name: &str) -> Option<&'a Schema> {
        self.flatten(s).find_map(|s| match s {
            Schema::Object { properties, .. } => properties
                .iter()
                .find(|p| p.name == name)
                .map(|p| &p.schema),
            _ => None,
        })
    }

    /// Whether property schema `p` allows strings, so that a discriminator
    /// mapping key can stand for itself.
    pub fn is_text(&self, p: &Schema) -> bool {
        match self.resolve(p) {
            Schema::Boolean | Schema::Integer { .. } | Schema::Number { .. } | Schema::Null => {
                false
            }
            Schema::Const { value, .. } => value.is_string(),
            Schema::Enum { values, .. } => values.iter().any(Value::is_string),
            Schema::Union {
                kind: UnionKind::Types,
                variants,
                ..
            } => variants.iter().any(|v| self.is_text(v)),
            _ => true,
        }
    }

    /// For a property schema `p` that does not allow strings, the value that
    /// discriminator mapping key `key` stands for.
    pub fn mapped_value(&self, p: &Schema, key: &str) -> Option<Value> {
        let value = serde_json::from_str::<Value>(key).ok()?;
        let allowed = match self.resolve(p) {
            Schema::Boolean => value.is_boolean(),
            Schema::Integer { .. } => value.is_i64() || value.is_u64(),
            Schema::Number { .. } => value.is_number(),
            Schema::Const { value: c, .. } => *c == value,
            Schema::Enum { values, .. } => values.contains(&value),
            Schema::Union { variants, .. } => {
                variants.iter().any(|v| self.mapped_value(v, key).is_some())
            }
            _ => false,
        };
        allowed.then_some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The keys and statuses of [`Operation::coverage`] for responses `keys`.
    fn coverage(keys: &[Status]) -> Vec<(String, Vec<u16>)> {
        let op = Operation {
            method: "GET".into(),
            operation_id: None,
            meta: Meta::default(),
            servers: Vec::new(),
            params: Vec::new(),
            querystring: None,
            body: None,
            responses: keys
                .iter()
                .map(|&status| Response {
                    status,
                    meta: Meta::default(),
                    content: Vec::new(),
                })
                .collect(),
            origin: Origin::default(),
            security: None,
        };
        op.coverage()
            .into_iter()
            .map(|(r, s)| (r.map_or("-".into(), |r| r.status.key()), s))
            .collect()
    }

    fn codes(range: std::ops::Range<u16>, except: &[u16]) -> Vec<u16> {
        range.filter(|s| !except.contains(s)).collect()
    }

    #[test]
    fn coverage_splits_statuses_by_key() {
        use Status::*;
        // Explicitly declared codes are excluded from a range; the rest are undeclared.
        assert_eq!(
            coverage(&[Code(200), Code(404), Range(4)]),
            vec![
                ("200".into(), vec![200]),
                ("404".into(), vec![404]),
                ("4XX".into(), codes(400..500, &[404])),
                (
                    "-".into(),
                    codes(200..600, &[200, 304])
                        .into_iter()
                        .filter(|s| !(400..500).contains(s))
                        .collect()
                ),
            ]
        );
        // `default` takes what is left, including 2xx statuses unless `2XX` covers them.
        assert_eq!(
            coverage(&[Range(2), Default]),
            vec![
                ("2XX".into(), codes(200..300, &[])),
                ("default".into(), codes(300..600, &[304]))
            ]
        );
        assert_eq!(
            coverage(&[Default]),
            vec![("default".into(), codes(200..600, &[304]))]
        );
        assert_eq!(coverage(&[]), vec![("-".into(), codes(200..600, &[304]))]);
        // 304 appears only where a key or range includes it, as 1xx statuses do.
        assert_eq!(
            coverage(&[Code(304), Default]),
            vec![
                ("304".into(), vec![304]),
                ("default".into(), codes(200..600, &[304]))
            ]
        );
        assert_eq!(
            coverage(&[Range(3), Default]),
            vec![
                ("3XX".into(), codes(300..400, &[])),
                (
                    "default".into(),
                    codes(200..600, &[])
                        .into_iter()
                        .filter(|s| !(300..400).contains(s))
                        .collect()
                )
            ]
        );
        // 1xx statuses appear only where a key names them.
        assert_eq!(
            coverage(&[Code(101), Range(1)]),
            vec![
                ("101".into(), vec![101]),
                ("1XX".into(), codes(100..200, &[101])),
                ("-".into(), codes(200..600, &[304])),
            ]
        );
    }
}
