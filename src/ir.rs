//! Language-neutral intermediate representation of an OpenAPI 3.2 document.
//!
//! Emitters for target languages consume this instead of the raw document, so
//! `$ref` resolution, version checks and validation live in one place.
//!
//! The IR is public so that other emitters can be built on it, but it is not
//! yet stable: expect breaking changes in 0.x releases.

use serde_json::Value;

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
    /// The component name, or for hoisted schemas the normalized JSON pointer
    /// of the `$ref` target (always starting with `#/`). Component names cannot
    /// start with `#`, so keys never collide.
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
    pub name: Option<String>,
    pub description: Option<String>,
    pub variables: Vec<ServerVariable>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerVariable {
    pub name: String,
    pub default: String,
    pub values: Vec<String>,
    pub description: Option<String>,
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
        bearer_format: Option<String>,
    },
    MutualTls,
    OAuth2 {
        metadata_url: Option<String>,
    },
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

#[derive(Debug, Clone, PartialEq)]
pub struct Content {
    pub media_type: String,
    pub schema: Option<Schema>,
    /// Schema of each item of a sequential media type (new in 3.2).
    pub item_schema: Option<Schema>,
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
        content_media_type: Option<String>,
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
    },
    Object {
        /// Includes names listed only in `required` (with schema `Any`).
        properties: Vec<Property>,
        /// `None` when `additionalProperties` is absent; `Some(Any)` when it
        /// is explicitly `true`. Both allow any extra property.
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
    /// Discriminating value -> schema key in [`Api::schemas`]. Includes the
    /// implicit mapping (component name -> component) for `Ref` variants.
    pub mapping: Vec<(String, String)>,
    pub default_mapping: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub name: String,
    pub required: bool,
    pub meta: Meta,
    pub schema: Schema,
}
