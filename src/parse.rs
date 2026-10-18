//! Builds the [`ir::Api`](crate::ir::Api) from loaded OpenAPI 3.2 documents.

use crate::Error;
use crate::documents::{Documents, Target};
use crate::ir::*;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::rc::Rc;
use url::Url;

const FIXED_METHODS: [&str; 9] = [
    "get", "put", "post", "delete", "options", "head", "patch", "trace", "query",
];

/// Methods that fetch (and most HTTP clients) normalize to uppercase, so an
/// `additionalOperations` entry like `post` would be sent as `POST`.
const NORMALIZED_METHODS: [&str; 6] = ["delete", "get", "head", "options", "post", "put"];

/// Header parameters the spec says SHALL be ignored.
const IGNORED_HEADERS: [&str; 3] = ["accept", "content-type", "authorization"];

/// Limit for literal schema nesting and `$ref` chains of non-schema objects.
/// Schema `$ref` targets are converted from a queue, so neither their cycles
/// nor their chains count toward this limit.
const MAX_DEPTH: usize = 64;

pub fn parse(docs: &Documents) -> Result<Api, Error> {
    let version = docs
        .root()
        .get("openapi")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::new("missing `openapi` version field"))?;
    if !version.starts_with("3.2.") {
        return Err(Error::new(format!(
            "unsupported OpenAPI version {version}: oasis supports 3.2.x only"
        )));
    }
    Parser::new(docs).api()
}

/// A value together with the base URI that references inside it resolve against.
#[derive(Clone)]
struct Node<'a> {
    v: &'a Value,
    base: Rc<Url>,
}

impl<'a> Node<'a> {
    fn get(&self, key: &str) -> Option<Node<'a>> {
        self.v.get(key).map(|v| self.at(v))
    }

    fn at(&self, v: &'a Value) -> Node<'a> {
        Node {
            v,
            base: self.base.clone(),
        }
    }

    fn str(&self, key: &str) -> Option<&'a str> {
        self.v.get(key).and_then(Value::as_str)
    }

    fn entries(&self) -> impl Iterator<Item = (&'a String, Node<'a>)> + '_ {
        self.v
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, v)| (k, self.at(v)))
    }

    /// [`Node::entries`] without `x-` specification extensions, for objects
    /// whose other keys are patterned (Paths, Responses).
    fn fields(&self) -> impl Iterator<Item = (&'a String, Node<'a>)> + '_ {
        self.entries().filter(|(k, _)| !k.starts_with("x-"))
    }

    fn items(&self) -> impl Iterator<Item = Node<'a>> + '_ {
        self.v.as_array().into_iter().flatten().map(|v| self.at(v))
    }
}

struct Parser<'a> {
    docs: &'a Documents,
    root: Node<'a>,
    /// Non-component `$ref` targets already queued for hoisting.
    hoisted: HashSet<Target>,
    /// Key and target of each hoisted schema, converted in this order.
    pending: Vec<(String, Node<'a>)>,
    operation_ids: HashSet<String>,
    security_schemes: Vec<SecurityScheme>,
}

impl<'a> Parser<'a> {
    fn new(docs: &'a Documents) -> Parser<'a> {
        Parser {
            docs,
            root: Node {
                v: docs.root(),
                base: docs.root_base(),
            },
            hoisted: HashSet::new(),
            pending: Vec::new(),
            operation_ids: HashSet::new(),
            security_schemes: Vec::new(),
        }
    }

    fn api(mut self) -> Result<Api, Error> {
        let root = self.root.clone();
        let info = root.v.get("info");
        let text = |key| {
            info.and_then(|i| i.get(key))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };

        let mut schemas = Vec::new();
        if let Some(map) = root.get("components").and_then(|c| c.get("schemas")) {
            for (name, s) in map.entries() {
                let schema = self
                    .schema(s.clone(), 0)
                    .map_err(|e| e.context(format!("components.schemas.{name}")))?;
                schemas.push(NamedSchema {
                    key: name.clone(),
                    hoisted: false,
                    meta: meta(s.v),
                    schema,
                });
            }
        }

        if let Some(map) = root
            .get("components")
            .and_then(|c| c.get("securitySchemes"))
        {
            for (name, s) in map.entries() {
                let s = self.deref(s)?;
                let kind = security_scheme(&s)
                    .map_err(|e| e.context(format!("components.securitySchemes.{name}")))?;
                self.security_schemes.push(SecurityScheme {
                    key: name.clone(),
                    meta: meta(s.v),
                    kind,
                });
            }
        }
        let security = match root.get("security") {
            Some(list) => self.security(&list).map_err(|e| e.context("security"))?,
            None => Vec::new(),
        };

        let mut paths = Vec::new();
        if let Some(map) = root.get("paths") {
            for (path, item) in map.fields() {
                if !path.starts_with('/') {
                    return Err(Error::new(format!(
                        "paths.{path}: path must start with `/`"
                    )));
                }
                let item = self
                    .path_item(path, item)
                    .map_err(|e| e.context(format!("paths.{path}")))?;
                paths.push(item);
            }
        }

        // Converting a target can hoist more, which this loop then reaches.
        let mut i = 0;
        while let Some((key, node)) = self.pending.get(i).cloned() {
            let schema = self
                .schema(node.clone(), 0)
                .map_err(|e| e.context(key.clone()))?;
            schemas.push(NamedSchema {
                key,
                hoisted: true,
                meta: meta(node.v),
                schema,
            });
            i += 1;
        }
        Ok(Api {
            title: text("title"),
            version: text("version"),
            servers: servers(root.v.get("servers"))?,
            schemas,
            paths,
            security_schemes: self.security_schemes,
            security,
        })
    }

    /// A `security` list of Security Requirement Objects.
    /// A malformed list is an error rather than read as fewer requirements,
    /// since `[]` and `{}` would then mean no or anonymous access.
    fn security(&mut self, list: &Node<'a>) -> Result<Vec<SecurityRequirement>, Error> {
        if !list.v.is_array() {
            return Err(Error::new("must be an array of security requirements"));
        }
        let mut out = Vec::new();
        for item in list.items() {
            if !item.v.is_object() {
                return Err(Error::new(format!(
                    "security requirement {} must map scheme names to scopes, e.g. `- bearer: []`",
                    item.v
                )));
            }
            let mut schemes = Vec::new();
            for (name, scopes) in item.entries() {
                let key = self.security_scheme_key(&item, name)?;
                let scopes = scopes
                    .v
                    .as_array()
                    .ok_or_else(|| Error::new(format!("scopes of `{name}` must be an array")))?
                    .iter()
                    .map(|s| {
                        s.as_str().map(String::from).ok_or_else(|| {
                            Error::new(format!("scopes of `{name}` must be strings"))
                        })
                    })
                    .collect::<Result<_, _>>()?;
                schemes.push((key, scopes));
            }
            out.push(SecurityRequirement { schemes });
        }
        Ok(out)
    }

    /// Security requirements name schemes in the entry document's
    /// `components.securitySchemes`, or reference them by URI.
    fn security_scheme_key(&mut self, from: &Node<'a>, name: &str) -> Result<String, Error> {
        if self.security_schemes.iter().any(|s| s.key == name) {
            return Ok(name.to_string());
        }
        if !name.contains(['#', '/']) {
            return Err(Error::new(format!("unknown security scheme `{name}`")));
        }
        let (loc, target) = self.lookup(from, name)?;
        if loc.doc == 0
            && let Some(component) = loc.pointer.strip_prefix("/components/securitySchemes/")
            && self.security_schemes.iter().any(|s| s.key == component)
        {
            return Ok(component.to_string());
        }
        let key = self.docs.display(&loc);
        if !self.security_schemes.iter().any(|s| s.key == key) {
            let target = self.deref(target)?;
            let kind = security_scheme(&target).map_err(|e| e.context(name.to_string()))?;
            self.security_schemes.push(SecurityScheme {
                key: key.clone(),
                meta: meta(target.v),
                kind,
            });
        }
        Ok(key)
    }

    /// Follows `$ref` chains of non-schema objects to their target.
    fn deref(&self, mut node: Node<'a>) -> Result<Node<'a>, Error> {
        for _ in 0..MAX_DEPTH {
            match node.str("$ref") {
                Some(r) => node = self.lookup(&node, r)?.1,
                None => return Ok(node),
            }
        }
        Err(Error::new("$ref chain too deep or cyclic"))
    }

    fn lookup(&self, from: &Node<'a>, reference: &str) -> Result<(Target, Node<'a>), Error> {
        let (loc, base) = self.docs.resolve(&from.base, reference)?;
        let v = self
            .docs
            .value(&loc)
            .expect("resolve checks the target exists");
        Ok((loc, Node { v, base }))
    }

    fn path_item(&mut self, path: &str, item: Node<'a>) -> Result<PathItem, Error> {
        let item = self.deref(item)?;
        let shared = item.get("parameters");

        let mut ops: Vec<(String, Node<'a>)> = Vec::new();
        for m in FIXED_METHODS {
            if let Some(op) = item.get(m) {
                ops.push((m.to_ascii_uppercase(), op));
            }
        }
        if let Some(extra) = item.get("additionalOperations") {
            for (method, op) in extra.entries() {
                let lower = method.to_ascii_lowercase();
                let fixed = FIXED_METHODS.contains(&lower.as_str());
                if fixed
                    && (NORMALIZED_METHODS.contains(&lower.as_str())
                        || *method == lower.to_ascii_uppercase())
                {
                    return Err(Error::new(format!(
                        "additionalOperations must not define `{method}`: it is sent as `{}`, use the `{lower}` field",
                        lower.to_ascii_uppercase()
                    )));
                }
                ops.push((method.clone(), op));
            }
        }

        let mut operations = Vec::new();
        for (method, op) in ops {
            let op = self
                .operation(path, &method, op, shared.clone())
                .map_err(|e| e.context(method.clone()))?;
            operations.push(op);
        }
        Ok(PathItem {
            path: path.to_string(),
            servers: servers(item.v.get("servers"))?,
            operations,
        })
    }

    fn operation(
        &mut self,
        path: &str,
        method: &str,
        op: Node<'a>,
        shared: Option<Node<'a>>,
    ) -> Result<Operation, Error> {
        let operation_id = op.str("operationId").map(String::from);
        if let Some(id) = &operation_id
            && !self.operation_ids.insert(id.clone())
        {
            return Err(Error::new(format!("duplicate operationId `{id}`")));
        }

        // Operation-level parameters override path-level ones with the same name and location.
        let mut raw: Vec<Node<'a>> = Vec::new();
        for list in [shared, op.get("parameters")].into_iter().flatten() {
            for p in list.items() {
                let p = self.deref(p)?;
                let key = |n: &Node<'a>| (n.str("name"), n.str("in"));
                raw.retain(|q| key(q) != key(&p));
                raw.push(p);
            }
        }

        let mut params = Vec::new();
        let mut querystring = None;
        for p in raw {
            let name = p.str("name").unwrap_or_default();
            if p.str("in") == Some("querystring") {
                if querystring.is_some() {
                    return Err(Error::new("more than one `in: querystring` parameter"));
                }
                querystring = Some(self.querystring(&p)?);
            } else if let Some(param) = self
                .param(&p)
                .map_err(|e| e.context(format!("parameter `{name}`")))?
            {
                params.push(param);
            }
        }
        if querystring.is_some() && params.iter().any(|p| p.location == Location::Query) {
            return Err(Error::new(
                "`in: querystring` cannot be combined with `in: query` parameters",
            ));
        }
        for name in path_template_names(path) {
            if !params
                .iter()
                .any(|p| p.location == Location::Path && p.name == name)
            {
                return Err(Error::new(format!(
                    "path parameter `{name}` is not defined"
                )));
            }
        }

        let body = match op.get("requestBody") {
            Some(b) => {
                let b = self.deref(b)?;
                Some(Body {
                    required: b
                        .v
                        .get("required")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    meta: meta(b.v),
                    content: self.content(b.get("content"))?,
                })
            }
            None => None,
        };

        let mut responses = Vec::new();
        if let Some(map) = op.get("responses") {
            for (key, r) in map.fields() {
                let r = self.deref(r)?;
                responses.push(Response {
                    status: status(key)?,
                    meta: meta(r.v),
                    content: self
                        .content(r.get("content"))
                        .map_err(|e| e.context(format!("response {key}")))?,
                });
            }
        }

        let security = match op.get("security") {
            Some(list) => Some(self.security(&list).map_err(|e| e.context("security"))?),
            None => None,
        };

        Ok(Operation {
            method: method.to_string(),
            operation_id,
            meta: meta(op.v),
            servers: servers(op.v.get("servers"))?,
            params,
            querystring,
            body,
            responses,
            security,
        })
    }

    fn param(&mut self, p: &Node<'a>) -> Result<Option<Param>, Error> {
        let name = p.str("name").ok_or_else(|| Error::new("missing `name`"))?;
        let location = match p.str("in") {
            Some("path") => Location::Path,
            Some("query") => Location::Query,
            Some("header") => Location::Header,
            Some("cookie") => Location::Cookie,
            other => return Err(Error::new(format!("invalid `in`: {other:?}"))),
        };
        if location == Location::Header
            && IGNORED_HEADERS.contains(&name.to_ascii_lowercase().as_str())
        {
            return Ok(None);
        }
        let required =
            p.v.get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        if location == Location::Path && !required {
            return Err(Error::new("path parameters must be `required: true`"));
        }

        let value = match (p.get("schema"), p.get("content")) {
            (Some(_), Some(_)) => {
                return Err(Error::new(
                    "parameters must not have both `schema` and `content`",
                ));
            }
            (None, Some(content)) => ParamValue::Content(self.single_content(content)?),
            (schema, None) => {
                let style = param_style(p.v, location)?;
                ParamValue::Schema {
                    style,
                    explode: p
                        .v
                        .get("explode")
                        .and_then(Value::as_bool)
                        .unwrap_or(matches!(style, Style::Form | Style::Cookie)),
                    allow_reserved: p
                        .v
                        .get("allowReserved")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    schema: match schema {
                        Some(s) => self.schema(s, 0)?,
                        None => Schema::Any,
                    },
                }
            }
        };

        Ok(Some(Param {
            name: name.to_string(),
            location,
            required,
            meta: meta(p.v),
            value,
        }))
    }

    fn querystring(&mut self, p: &Node<'a>) -> Result<QueryString, Error> {
        let content = p
            .get("content")
            .ok_or_else(|| Error::new("`in: querystring` parameter must use `content`"))?;
        Ok(QueryString {
            required: p
                .v
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            meta: meta(p.v),
            content: self.single_content(content)?,
        })
    }

    fn single_content(&mut self, content: Node<'a>) -> Result<Content, Error> {
        let mut list = self.content(Some(content))?;
        if list.len() != 1 {
            return Err(Error::new("`content` must contain exactly one media type"));
        }
        Ok(list.remove(0))
    }

    fn content(&mut self, content: Option<Node<'a>>) -> Result<Vec<Content>, Error> {
        let Some(map) = content else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for (media_type, m) in map.entries() {
            let m = self.deref(m)?;
            let mut schema = |key| m.get(key).map(|s| self.schema(s, 0)).transpose();
            out.push(Content {
                media_type: media_type.clone(),
                schema: schema("schema")?,
                item_schema: schema("itemSchema")?,
            });
        }
        Ok(out)
    }

    /// Resolves a schema `$ref` to its key in [`Api::schemas`], hoisting
    /// targets other than the entry document's `components.schemas` once.
    /// Hoisted targets are converted later, so a long `$ref` chain does not
    /// recurse.
    fn schema_ref(&mut self, from: &Node<'a>, reference: &str) -> Result<String, Error> {
        let (loc, target) = self.lookup(from, reference)?;
        if loc.doc == 0
            && let Some(name) = loc.pointer.strip_prefix("/components/schemas/")
            && !name.contains('/')
        {
            return Ok(name.replace("~1", "/").replace("~0", "~"));
        }

        let key = self.docs.display(&loc);
        // Queue each target once; later references, cycles included, reuse the key.
        if self.hoisted.insert(loc) {
            self.pending.push((key.clone(), target));
        }
        Ok(key)
    }

    fn schema(&mut self, node: Node<'a>, depth: usize) -> Result<Schema, Error> {
        if depth > MAX_DEPTH {
            return Err(Error::new("schema nesting too deep"));
        }
        let obj = match node.v {
            Value::Bool(true) => return Ok(Schema::Any),
            Value::Bool(false) => return Ok(Schema::Never),
            Value::Object(o) => o,
            _ => return Err(Error::new("schema must be an object or boolean")),
        };
        // `$id` sets the base URI for this schema and everything inside it.
        let node = match node.str("$id").map(|id| node.base.join(id)) {
            Some(Ok(base)) => Node {
                v: node.v,
                base: Rc::new(base),
            },
            Some(Err(e)) => return Err(Error::new(format!("invalid `$id`: {e}"))),
            None => node,
        };

        let mut parts = Vec::new();
        // In JSON Schema 2020-12, keywords next to `$ref` also apply.
        if let Some(r) = node.str("$ref") {
            parts.push(Schema::Ref(self.schema_ref(&node, r)?));
        }
        if let Some(base) = self.base_schema(&node, obj, depth)? {
            parts.push(base);
        }
        if let Some(all) = node.get("allOf") {
            for s in all.items() {
                parts.push(self.schema(s, depth + 1)?);
            }
        }
        for (key, kind) in [("anyOf", UnionKind::AnyOf), ("oneOf", UnionKind::OneOf)] {
            if let Some(list) = node.get(key) {
                let variants = list
                    .items()
                    .map(|s| self.schema(s, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;
                let discriminator = match node.get("discriminator") {
                    Some(d) => Some(self.discriminator(&d, &variants)?),
                    None => None,
                };
                parts.push(Schema::Union {
                    kind,
                    variants,
                    discriminator,
                });
            }
        }
        parts.retain(|s| *s != Schema::Any);
        Ok(match parts.len() {
            0 => Schema::Any,
            1 => parts.remove(0),
            _ => Schema::Intersection(parts),
        })
    }

    fn discriminator(&mut self, d: &Node<'a>, variants: &[Schema]) -> Result<Discriminator, Error> {
        let property_name = d
            .str("propertyName")
            .ok_or_else(|| Error::new("discriminator requires `propertyName`"))?
            .to_string();
        // Values are component names of the entry document, or URI references.
        let components = self.root.v.pointer("/components/schemas");
        let mut target = |s: &str| {
            if components.is_some_and(|c| c.get(s).is_some()) {
                Ok(s.to_string())
            } else {
                self.schema_ref(d, s)
            }
        };
        let mut mapping = Vec::new();
        if let Some(map) = d.v.get("mapping").and_then(Value::as_object) {
            for (value, t) in map {
                let t = t
                    .as_str()
                    .ok_or_else(|| Error::new("discriminator mapping values must be strings"))?;
                mapping.push((value.clone(), target(t)?));
            }
        }
        let default_mapping = d.str("defaultMapping").map(&mut target).transpose()?;
        // Values not in `mapping` are assumed to be component names.
        for v in variants {
            if let Schema::Ref(name) = v
                && !name.contains('#')
                && !mapping.iter().any(|(value, _)| value == name)
            {
                mapping.push((name.clone(), name.clone()));
            }
        }
        Ok(Discriminator {
            property_name,
            mapping,
            default_mapping,
        })
    }

    /// The part of a schema described by `const`, `enum` and `type`, without composition.
    fn base_schema(
        &mut self,
        node: &Node<'a>,
        obj: &'a Map<String, Value>,
        depth: usize,
    ) -> Result<Option<Schema>, Error> {
        let types: Vec<&str> = match obj.get("type") {
            Some(Value::String(t)) => vec![t],
            Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).collect(),
            Some(_) => return Err(Error::new("`type` must be a string or array")),
            None if ["properties", "additionalProperties", "required"]
                .iter()
                .any(|k| obj.contains_key(*k)) =>
            {
                vec!["object"]
            }
            None if ["items", "prefixItems"]
                .iter()
                .any(|k| obj.contains_key(*k)) =>
            {
                vec!["array"]
            }
            None => vec![],
        };
        let mut variants = types
            .into_iter()
            .map(|t| self.typed_schema(t, node, obj, depth))
            .collect::<Result<Vec<_>, _>>()?;
        let typed = match variants.len() {
            0 => None,
            1 => Some(variants.remove(0)),
            _ => Some(Schema::Union {
                kind: UnionKind::Types,
                variants,
                discriminator: None,
            }),
        };

        let base = || typed.clone().map(Box::new);
        if let Some(c) = obj.get("const") {
            return Ok(Some(Schema::Const {
                value: c.clone(),
                base: base(),
            }));
        }
        if let Some(e) = obj.get("enum").and_then(Value::as_array) {
            return Ok(Some(Schema::Enum {
                values: e.clone(),
                base: base(),
            }));
        }
        Ok(typed)
    }

    fn typed_schema(
        &mut self,
        t: &str,
        node: &Node<'a>,
        obj: &'a Map<String, Value>,
        depth: usize,
    ) -> Result<Schema, Error> {
        let text = |key| obj.get(key).and_then(Value::as_str).map(String::from);
        Ok(match t {
            "null" => Schema::Null,
            "boolean" => Schema::Boolean,
            "integer" => Schema::Integer {
                format: text("format"),
            },
            "number" => Schema::Number {
                format: text("format"),
            },
            "string" => Schema::String {
                format: text("format"),
                content_media_type: text("contentMediaType"),
                content_encoding: text("contentEncoding"),
            },
            "array" => {
                let prefix_items = match node.get("prefixItems") {
                    Some(list) => list
                        .items()
                        .map(|s| self.schema(s, depth + 1))
                        .collect::<Result<_, _>>()?,
                    None => Vec::new(),
                };
                let items = match node.get("items") {
                    Some(s) => Some(Box::new(self.schema(s, depth + 1)?)),
                    None => None,
                };
                Schema::Array {
                    prefix_items,
                    items,
                }
            }
            "object" => {
                let required: Vec<&str> = obj
                    .get("required")
                    .and_then(Value::as_array)
                    .map(|r| r.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                let mut properties = Vec::new();
                if let Some(props) = node.get("properties") {
                    for (name, s) in props.entries() {
                        properties.push(Property {
                            name: name.clone(),
                            required: required.contains(&name.as_str()),
                            meta: meta(s.v),
                            schema: self.schema(s, depth + 1)?,
                        });
                    }
                }
                // Required names without a property schema must still be present.
                for name in required {
                    if !properties.iter().any(|p| p.name == name) {
                        properties.push(Property {
                            name: name.to_string(),
                            required: true,
                            meta: Meta::default(),
                            schema: Schema::Any,
                        });
                    }
                }
                let additional = match node.get("additionalProperties") {
                    Some(s) => Some(Box::new(self.schema(s, depth + 1)?)),
                    None => None,
                };
                Schema::Object {
                    properties,
                    additional,
                }
            }
            other => return Err(Error::new(format!("unknown type `{other}`"))),
        })
    }
}

fn param_style(p: &Value, location: Location) -> Result<Style, Error> {
    let name = p.get("style").and_then(Value::as_str);
    let style = match (location, name) {
        (Location::Query, None | Some("form")) => Style::Form,
        (Location::Query, Some("deepObject")) => Style::DeepObject,
        (Location::Path | Location::Header, None | Some("simple")) => Style::Simple,
        (Location::Cookie, None | Some("form")) => Style::Form,
        (Location::Cookie, Some("cookie")) => Style::Cookie,
        (_, Some(s)) => {
            return Err(Error::new(format!(
                "style `{s}` for {} parameters is not supported",
                location.as_str()
            )));
        }
    };
    Ok(style)
}

fn servers(v: Option<&Value>) -> Result<Vec<Server>, Error> {
    let mut out = Vec::new();
    for s in v.and_then(Value::as_array).into_iter().flatten() {
        let text = |v: &Value, key| v.get(key).and_then(Value::as_str).map(String::from);
        let url = text(s, "url").ok_or_else(|| Error::new("server requires `url`"))?;
        let mut variables = Vec::new();
        for (name, var) in s
            .get("variables")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            variables.push(ServerVariable {
                name: name.clone(),
                default: text(var, "default").ok_or_else(|| {
                    Error::new(format!("server variable `{name}` requires `default`"))
                })?,
                values: var
                    .get("enum")
                    .and_then(Value::as_array)
                    .map(|e| {
                        e.iter()
                            .filter_map(Value::as_str)
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default(),
                description: text(var, "description"),
            });
        }
        out.push(Server {
            url,
            name: text(s, "name"),
            description: text(s, "description"),
            variables,
        });
    }
    Ok(out)
}

fn security_scheme(s: &Node) -> Result<SecuritySchemeKind, Error> {
    let required = |key: &str| {
        s.str(key)
            .map(String::from)
            .ok_or_else(|| Error::new(format!("security scheme requires `{key}`")))
    };
    Ok(match s.str("type") {
        Some("apiKey") => SecuritySchemeKind::ApiKey {
            name: required("name")?,
            location: match s.str("in") {
                Some("query") => ApiKeyLocation::Query,
                Some("header") => ApiKeyLocation::Header,
                Some("cookie") => ApiKeyLocation::Cookie,
                other => return Err(Error::new(format!("invalid apiKey `in`: {other:?}"))),
            },
        },
        Some("http") => SecuritySchemeKind::Http {
            scheme: required("scheme")?.to_ascii_lowercase(),
            bearer_format: s.str("bearerFormat").map(String::from),
        },
        Some("mutualTLS") => SecuritySchemeKind::MutualTls,
        Some("oauth2") => SecuritySchemeKind::OAuth2 {
            metadata_url: s.str("oauth2MetadataUrl").map(String::from),
        },
        Some("openIdConnect") => SecuritySchemeKind::OpenIdConnect {
            url: required("openIdConnectUrl")?,
        },
        other => {
            return Err(Error::new(format!(
                "invalid security scheme type: {other:?}"
            )));
        }
    })
}

fn status(key: &str) -> Result<Status, Error> {
    let b = key.as_bytes();
    match b {
        b"default" => Ok(Status::Default),
        [d @ b'1'..=b'5', b'X', b'X'] => Ok(Status::Range(d - b'0')),
        [b'1'..=b'5', b'0'..=b'9', b'0'..=b'9'] => Ok(Status::Code(key.parse().unwrap())),
        _ => Err(Error::new(format!(
            "invalid response key `{key}`: expected a status code, `1XX`..`5XX` or `default`"
        ))),
    }
}

fn meta(v: &Value) -> Meta {
    let text = |key| v.get(key).and_then(Value::as_str).map(String::from);
    let flag = |key| v.get(key).and_then(Value::as_bool).unwrap_or(false);
    Meta {
        title: text("title"),
        summary: text("summary"),
        description: text("description"),
        deprecated: flag("deprecated"),
        read_only: flag("readOnly"),
        write_only: flag("writeOnly"),
        default: v.get("default").cloned(),
        extensions: v
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(k, _)| k.starts_with("x-"))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    }
}

fn path_template_names(path: &str) -> Vec<&str> {
    path.split('{')
        .skip(1)
        .filter_map(|s| s.split_once('}').map(|(name, _)| name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse_value(doc: Value) -> Result<Api, Error> {
        parse(&Documents::from_value(doc)?)
    }

    fn api(paths: Value, schemas: Value) -> Result<Api, Error> {
        parse_value(json!({
            "openapi": "3.2.0",
            "paths": paths,
            "components": {"schemas": schemas}
        }))
    }

    /// Converts one schema, placed at `components.schemas.S`.
    fn schema(v: Value) -> Schema {
        api(json!({}), json!({ "S": v }))
            .unwrap()
            .schemas
            .remove(0)
            .schema
    }

    fn string() -> Schema {
        Schema::String {
            format: None,
            content_media_type: None,
            content_encoding: None,
        }
    }

    fn get(params: Value) -> Value {
        json!({"/s": {"get": {"parameters": params, "responses": {}}}})
    }

    /// Asserts that `doc` is rejected with a message containing `needle`.
    fn rejects(doc: Value, needle: &str) {
        let err = parse_value(doc).unwrap_err().to_string();
        assert!(err.contains(needle), "expected {needle:?} in {err:?}");
    }

    #[test]
    fn rejects_other_versions() {
        let err = parse_value(json!({"openapi": "3.1.0"})).unwrap_err();
        assert!(err.to_string().contains("3.2.x only"));
    }

    #[test]
    fn type_array_becomes_union() {
        assert_eq!(
            schema(json!({"type": ["string", "null"]})),
            Schema::Union {
                kind: UnionKind::Types,
                variants: vec![string(), Schema::Null],
                discriminator: None
            }
        );
    }

    #[test]
    fn enum_keeps_its_type() {
        let s = schema(json!({"type": "integer", "format": "int64", "enum": [1, 2]}));
        assert_eq!(
            s,
            Schema::Enum {
                values: vec![json!(1), json!(2)],
                base: Some(Box::new(Schema::Integer {
                    format: Some("int64".into())
                }))
            }
        );
    }

    #[test]
    fn ref_with_siblings_is_intersection() {
        let api = api(
            json!({}),
            json!({
                "A": {"type": "object"},
                "B": {"$ref": "#/components/schemas/A", "properties": {"x": {"type": "integer"}}}
            }),
        )
        .unwrap();
        assert!(
            matches!(&api.schemas[1].schema, Schema::Intersection(v) if v[0] == Schema::Ref("A".into()))
        );
    }

    #[test]
    fn required_without_properties_is_kept() {
        let s = schema(json!({"allOf": [true], "required": ["id"]}));
        let Schema::Object { properties, .. } = s else {
            panic!("{s:?}")
        };
        assert_eq!(properties[0].name, "id");
        assert!(properties[0].required);
    }

    #[test]
    fn hoists_recursive_non_component_refs() {
        let api = api(
            json!({}),
            json!({"Tree": {
                "$defs": {"node": {"type": "object", "properties": {
                    "left": {"$ref": "#/components/schemas/Tree/$defs/node"},
                    "right": {"$ref": "#/components/schemas/Tree/%24defs/node"}
                }}},
                "$ref": "#/components/schemas/Tree/$defs/node"
            }}),
        )
        .unwrap();
        let key = "#/components/schemas/Tree/$defs/node";
        assert_eq!(api.schemas[0].schema, Schema::Ref(key.into()));
        assert_eq!(api.schemas.len(), 2);
        assert_eq!(api.schemas[1].key, key);
        assert!(api.schemas[1].hoisted);
    }

    #[test]
    fn long_non_component_ref_chain_does_not_recurse() {
        const N: usize = 5000;
        let defs: serde_json::Map<String, Value> = (0..N)
            .map(|i| {
                let next = json!({"$ref": format!("#/components/schemas/S/$defs/d{}", i + 1)});
                (format!("d{i}"), next)
            })
            .chain([(format!("d{N}"), json!({"type": "string"}))])
            .collect();
        let api = api(
            json!({}),
            json!({"S": {"$ref": "#/components/schemas/S/$defs/d0", "$defs": defs}}),
        )
        .unwrap();
        assert_eq!(api.schemas.len(), N + 2);
        assert_eq!(api.schemas[N + 1].schema, string());
    }

    #[test]
    fn schema_ids_change_the_base_uri() {
        let s = schema(json!({
            "$id": "https://example.com/schemas/s",
            "properties": {"a": {"$ref": "s#/$defs/a"}},
            "$defs": {"a": {"type": "string"}}
        }));
        let Schema::Object { properties, .. } = s else {
            panic!("{s:?}")
        };
        assert_eq!(
            properties[0].schema,
            Schema::Ref("#/components/schemas/S/$defs/a".into())
        );
    }

    #[test]
    fn discriminator_includes_implicit_mapping() {
        let api = api(
            json!({}),
            json!({
                "A": {"type": "object"},
                "B": {"type": "object"},
                "U": {
                    "oneOf": [{"$ref": "#/components/schemas/A"}, {"$ref": "#/components/schemas/B"}],
                    "discriminator": {"propertyName": "t", "mapping": {"a": "#/components/schemas/A", "b": "B"}}
                }
            }),
        )
        .unwrap();
        let Schema::Union {
            kind,
            discriminator: Some(d),
            ..
        } = &api.schemas[2].schema
        else {
            panic!()
        };
        assert_eq!(*kind, UnionKind::OneOf);
        assert_eq!(
            d.mapping,
            vec![
                ("a".into(), "A".into()),
                ("b".into(), "B".into()),
                ("A".into(), "A".into()),
                ("B".into(), "B".into())
            ]
        );
    }

    #[test]
    fn parses_status_keys() {
        assert_eq!(status("200").unwrap(), Status::Code(200));
        assert_eq!(status("4XX").unwrap(), Status::Range(4));
        assert_eq!(status("default").unwrap(), Status::Default);
        for bad in ["2xx", "6XX", "20", "abc"] {
            let err = status(bad).unwrap_err().to_string();
            assert!(err.contains("invalid response key"), "{bad}: {err}");
        }
    }

    #[test]
    fn param_defaults_and_explode() {
        let api = api(
            get(json!([
                {"name": "a", "in": "query"},
                {"name": "b", "in": "cookie", "style": "cookie", "explode": false},
                {"name": "c", "in": "header", "explode": true}
            ])),
            json!({}),
        )
        .unwrap();
        let styles: Vec<_> = api.paths[0].operations[0]
            .params
            .iter()
            .map(|p| match &p.value {
                ParamValue::Schema { style, explode, .. } => (*style, *explode),
                _ => panic!(),
            })
            .collect();
        assert_eq!(
            styles,
            vec![
                (Style::Form, true),
                (Style::Cookie, false),
                (Style::Simple, true)
            ]
        );
    }

    #[test]
    fn rejects_invalid_params() {
        let cases = [
            (
                json!([{"name": "q", "in": "querystring", "content": {"application/x-www-form-urlencoded": {}}}, {"name": "x", "in": "query"}]),
                "cannot be combined with `in: query`",
            ),
            (
                json!([{"name": "x", "in": "query", "schema": {}, "content": {"text/plain": {}}}]),
                "both `schema` and `content`",
            ),
            (
                json!([{"name": "x", "in": "query", "style": "pipeDelimited"}]),
                "style `pipeDelimited` for query parameters is not supported",
            ),
            (
                json!([{"name": "a", "in": "querystring", "content": {"text/plain": {}}}, {"name": "b", "in": "querystring", "content": {"text/plain": {}}}]),
                "more than one `in: querystring`",
            ),
            (
                json!([{"name": "id", "in": "path"}]),
                "must be `required: true`",
            ),
            (json!([{"in": "query"}]), "missing `name`"),
            (json!([{"name": "x", "in": "body"}]), "invalid `in`"),
            (
                json!([{"name": "q", "in": "querystring", "schema": {}}]),
                "must use `content`",
            ),
            (
                json!([{"name": "x", "in": "query", "content": {"a/b": {}, "c/d": {}}}]),
                "exactly one media type",
            ),
        ];
        for (params, needle) in cases {
            rejects(json!({"openapi": "3.2.0", "paths": get(params)}), needle);
        }
    }

    #[test]
    fn rejects_malformed_documents() {
        let with_schema =
            |s: Value| json!({"openapi": "3.2.0", "components": {"schemas": {"S": s}}});
        let with_scheme =
            |s: Value| json!({"openapi": "3.2.0", "components": {"securitySchemes": {"k": s}}});
        let mut deep = json!({"type": "string"});
        for _ in 0..=MAX_DEPTH {
            deep = json!({"type": "array", "items": deep});
        }
        let cases = [
            (json!({"paths": {}}), "missing `openapi`"),
            (
                json!({
                    "openapi": "3.2.0",
                    "security": [{"k": "read"}],
                    "components": {"securitySchemes": {"k": {"type": "apiKey", "in": "header", "name": "K"}}}
                }),
                "scopes of `k` must be an array",
            ),
            (
                json!({"openapi": "3.2.0", "paths": {"/a": {"$ref": "#/paths/~1b"}, "/b": {"$ref": "#/paths/~1a"}}}),
                "too deep or cyclic",
            ),
            (
                json!({"openapi": "3.2.0", "paths": {"/p/{id}": {"get": {"responses": {}}}}}),
                "path parameter `id` is not defined",
            ),
            (with_schema(deep), "nesting too deep"),
            (with_schema(json!(1)), "must be an object or boolean"),
            (with_schema(json!({"$id": "http://"})), "invalid `$id`"),
            (
                with_schema(json!({"oneOf": [{"type": "string"}], "discriminator": {}})),
                "requires `propertyName`",
            ),
            (
                with_schema(
                    json!({"oneOf": [{"type": "string"}], "discriminator": {"propertyName": "t", "mapping": {"a": 1}}}),
                ),
                "mapping values must be strings",
            ),
            (
                with_schema(json!({"type": 1})),
                "`type` must be a string or array",
            ),
            (with_schema(json!({"type": "str"})), "unknown type `str`"),
            (
                json!({"openapi": "3.2.0", "servers": [{}]}),
                "server requires `url`",
            ),
            (
                json!({"openapi": "3.2.0", "servers": [{"url": "http://{h}", "variables": {"h": {}}}]}),
                "server variable `h` requires `default`",
            ),
            (
                with_scheme(json!({"type": "apiKey", "in": "header"})),
                "requires `name`",
            ),
            (
                with_scheme(json!({"type": "apiKey", "in": "body", "name": "K"})),
                "invalid apiKey `in`",
            ),
            (with_scheme(json!({"type": "http"})), "requires `scheme`"),
            (
                with_scheme(json!({"type": "openIdConnect"})),
                "requires `openIdConnectUrl`",
            ),
            (
                with_scheme(json!({"type": "basic"})),
                "invalid security scheme type",
            ),
        ];
        for (doc, needle) in cases {
            rejects(doc, needle);
        }
    }

    #[test]
    fn drops_ignored_headers_and_overrides_path_level_params() {
        let paths = json!({"/s": {
            "parameters": [
                {"name": "a", "in": "query", "schema": {"type": "string"}},
                {"name": "Authorization", "in": "header"}
            ],
            "get": {
                "parameters": [
                    {"name": "a", "in": "query", "schema": {"type": "integer"}},
                    {"name": "Content-Type", "in": "header"},
                    {"name": "X-Id", "in": "header"}
                ],
                "responses": {}
            }
        }});
        let api = api(paths, json!({})).unwrap();
        let params: Vec<(&str, &Schema)> = api.paths[0].operations[0]
            .params
            .iter()
            .map(|p| match &p.value {
                ParamValue::Schema { schema, .. } => (p.name.as_str(), schema),
                _ => panic!(),
            })
            .collect();
        assert_eq!(
            params,
            vec![
                ("a", &Schema::Integer { format: None }),
                ("X-Id", &Schema::Any)
            ]
        );
    }

    #[test]
    fn rejects_duplicate_operation_ids() {
        let paths = json!({
            "/a": {"get": {"operationId": "x", "responses": {}}},
            "/b": {"get": {"operationId": "x", "responses": {}}}
        });
        assert!(
            api(paths, json!({}))
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
    }

    #[test]
    fn parses_security() {
        let api = parse_value(json!({
            "openapi": "3.2.0",
            "security": [{"key": []}],
            "paths": {"/a": {
                "get": {"responses": {}},
                "post": {"security": [], "responses": {}},
                "put": {"security": [{}, {"#/components/securitySchemes/oauth": ["read"]}], "responses": {}}
            }},
            "components": {"securitySchemes": {
                "key": {"type": "apiKey", "in": "header", "name": "X-Key"},
                "oauth": {"type": "oauth2", "flows": {}}
            }}
        }))
        .unwrap();
        let req = |schemes: &[(&str, &[&str])]| SecurityRequirement {
            schemes: schemes
                .iter()
                .map(|(n, s)| (n.to_string(), s.iter().map(|s| s.to_string()).collect()))
                .collect(),
        };
        assert_eq!(api.security, vec![req(&[("key", &[])])]);
        let ops = &api.paths[0].operations;
        assert_eq!(ops[0].security, None);
        assert_eq!(ops[2].security, Some(vec![])); // post
        assert_eq!(
            ops[1].security, // put
            Some(vec![req(&[]), req(&[("oauth", &["read"])])])
        );
        assert_eq!(api.security_schemes.len(), 2);
        assert_eq!(
            api.security_schemes[0].kind,
            SecuritySchemeKind::ApiKey {
                name: "X-Key".into(),
                location: ApiKeyLocation::Header
            }
        );
    }

    #[test]
    fn rejects_unknown_security_schemes() {
        let err = parse_value(json!({"openapi": "3.2.0", "security": [{"nope": []}]})).unwrap_err();
        assert!(err.to_string().contains("unknown security scheme"), "{err}");
    }

    #[test]
    fn rejects_malformed_security() {
        let schemes = json!({"bearer": {"type": "http", "scheme": "bearer"}});
        for security in [
            json!(["bearer"]),
            json!({"bearer": []}),
            json!([{"bearer": [1]}]),
        ] {
            let err = parse_value(json!({
                "openapi": "3.2.0",
                "security": security,
                "components": {"securitySchemes": schemes}
            }))
            .unwrap_err();
            assert!(
                err.to_string().starts_with("security: "),
                "{security}: {err}"
            );
        }
    }

    #[test]
    fn skips_extensions_in_paths_and_responses() {
        let paths = json!({
            "x-note": {"get": {"responses": {}}},
            "/a": {"get": {"responses": {"200": {"description": "ok"}, "x-codegen": {"a": 1}}}}
        });
        let api = api(paths, json!({})).unwrap();
        assert_eq!(api.paths.len(), 1);
        assert_eq!(api.paths[0].operations[0].responses.len(), 1);
    }

    #[test]
    fn rejects_paths_without_leading_slash() {
        let err = api(json!({"pets": {}}), json!({})).unwrap_err();
        assert!(err.to_string().contains("must start with `/`"), "{err}");
    }

    #[test]
    fn additional_operations_cannot_shadow_fixed_methods() {
        for method in ["POST", "post", "PATCH"] {
            let paths = json!({"/s": {"additionalOperations": {method: {"responses": {}}}}});
            rejects(
                json!({"openapi": "3.2.0", "paths": paths}),
                "additionalOperations must not define",
            );
        }
        let paths = json!({"/s": {"additionalOperations": {"LINK": {"responses": {}}}}});
        assert!(api(paths, json!({})).is_ok());
    }
}
