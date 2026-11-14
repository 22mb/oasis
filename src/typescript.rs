//! TypeScript emitter: one self-contained file with types and a fetch-based runtime.

mod ast;

use crate::Error;
use crate::ir::*;
use ast::{Expr, Item, Member, Type, quote};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

const RUNTIME: &str = include_str!("typescript/runtime.ts");

/// Methods that `fetch` normalizes to uppercase, so an `additionalOperations`
/// entry like `post` would be sent as `POST`.
const NORMALIZED_METHODS: [&str; 6] = ["delete", "get", "head", "options", "post", "put"];

pub fn emit(api: &Api) -> Result<String, Error> {
    check(api)?;
    Emitter::new(api).emit()
}

/// Rejects what the document allows but the client cannot do.
fn check(api: &Api) -> Result<(), Error> {
    for item in &api.paths {
        for op in &item.operations {
            check_operation(op).map_err(|e| e.context(format!("paths.{}", item.path)))?;
        }
    }
    Ok(())
}

fn check_operation(op: &Operation) -> Result<(), Error> {
    check_method(&op.method).map_err(|e| e.about_origin(op.origin))?;
    for c in op.body.iter().flat_map(|b| &b.content) {
        if essence(&c.media_type) != "multipart/form-data" {
            continue;
        }
        for e in &c.encoding {
            check_part(e).map_err(|err| {
                err.about_origin(e.origin)
                    .context(format!("encoding.{}", e.name))
                    .context(c.media_type.clone())
                    .context("requestBody")
                    .context(op.method.clone())
            })?;
        }
    }
    Ok(())
}

/// A `multipart/form-data` body is sent as `FormData`, which cannot carry
/// part headers or nested parts. Optional headers are left out.
fn check_part(e: &Encoding) -> Result<(), Error> {
    if let Some(h) = e.headers.iter().find(|h| h.required) {
        return Err(Error::new(format!(
            "required part header `{}` is not supported",
            h.name
        )));
    }
    if let Some(Serialization::ContentType(ct)) = &e.serialization
        && ct.split(',').any(|t| essence(t).starts_with("multipart/"))
    {
        return Err(Error::new("nested multipart parts are not supported"));
    }
    Ok(())
}

/// The client defines a property for each method alongside `with` and must
/// not be thenable, so neither name can be a method. Fixed methods are
/// uppercase in the IR, so any other spelling comes from
/// `additionalOperations`.
fn check_method(method: &str) -> Result<(), Error> {
    if matches!(method, "with" | "then") {
        return Err(Error::new(format!(
            "additionalOperations must not define `{method}`: the generated client reserves that name"
        )));
    }
    let (lower, upper) = (method.to_ascii_lowercase(), method.to_ascii_uppercase());
    if method != upper && NORMALIZED_METHODS.contains(&lower.as_str()) {
        return Err(Error::new(format!(
            "additionalOperations must not define `{method}`: it is sent as `{upper}`; use the `{lower}` field instead"
        )));
    }
    Ok(())
}

/// Which side of the wire a type describes. `readOnly` properties are absent
/// (`?: never`) from requests, and `writeOnly` properties from responses.
#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Request,
    Response,
}

struct Emitter<'a> {
    api: &'a Api,
    schemas: Schemas<'a>,
    /// The type alias of each schema; its request variant adds `$Request`.
    names: HashMap<&'a str, String>,
    /// Schemas whose request view differs from their response view.
    request_variants: HashSet<&'a str>,
}

impl<'a> Emitter<'a> {
    fn new(api: &'a Api) -> Emitter<'a> {
        let schemas = Schemas::new(&api.schemas);

        // `$` cannot come from a key, so `$Pet$Request` never collides with a
        // schema named `Pet$Request`; names that do collide are numbered.
        let mut names = HashMap::new();
        let mut taken = HashSet::new();
        for s in &api.schemas {
            let name = unique(format!("${}", identifier(&s.key)), &mut taken);
            names.insert(s.key.as_str(), name);
        }

        // A schema needs a request variant if it has a readOnly or writeOnly
        // property, or references a schema that does.
        let mut direct = Vec::new();
        for s in &api.schemas {
            let mut flagged = false;
            let mut refs = Vec::new();
            s.schema.walk(&mut |node| match node {
                Schema::Ref(key) => refs.push(key.as_str()),
                Schema::Object { properties, .. } => {
                    flagged |= properties
                        .iter()
                        .any(|p| p.meta.read_only || p.meta.write_only);
                }
                _ => {}
            });
            direct.push((s.key.as_str(), flagged, refs));
        }
        let mut request_variants: HashSet<&str> = direct
            .iter()
            .filter(|(_, flagged, _)| *flagged)
            .map(|(key, ..)| *key)
            .collect();
        loop {
            let before = request_variants.len();
            for (key, _, refs) in &direct {
                if refs.iter().any(|r| request_variants.contains(r)) {
                    request_variants.insert(key);
                }
            }
            if request_variants.len() == before {
                break;
            }
        }

        Emitter {
            api,
            schemas,
            names,
            request_variants,
        }
    }

    fn emit(&self) -> Result<String, Error> {
        let api = self.api;
        let source = [api.title.as_str(), api.version.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        let mut items = vec![
            Item::Comment(vec![format!(
                "Generated by oasis-gen from {}. Do not edit.",
                quote(&source)
            )]),
            Item::Blank,
        ];

        // Build each operation's type and runtime data in one pass so both are
        // based on the same data.
        let mut statuses = BTreeMap::new();
        let mut methods: Vec<&str> = Vec::new();
        let mut data = Vec::new();
        let mut paths = Vec::new();
        for item in &api.paths {
            let mut operations = Vec::new();
            for op in &item.operations {
                if !methods.contains(&op.method.as_str()) {
                    methods.push(&op.method);
                }
                let (member, info) = self.operation(item, op, &mut statuses);
                operations.push(member);
                data.extend(info);
            }
            paths.push(Member::new(&item.path, false, Type::object(operations)));
        }
        items.push(Item::Interface {
            name: "paths".into(),
            members: paths,
        });
        items.push(Item::Blank);
        if !statuses.is_empty() {
            items.push(Item::Comment(vec![
                "The statuses each response key covers, as `[successes, failures]`, then".into(),
                "any of 204, 205 and 304 that a range or `default` covers, which have no".into(),
                "body. The operations with the same keys share one.".into(),
            ]));
            let runs = status_runs(&statuses);
            items.extend(statuses.into_iter().map(|(name, ty)| Item::alias(name, ty)));
            items.extend(runs);
            items.push(Item::Blank);
        }

        items.push(Item::Interface {
            name: "components".into(),
            members: vec![
                self.schema_group("schemas", View::Response),
                self.schema_group("requestSchemas", View::Request),
            ],
        });
        items.push(Item::Blank);
        items.push(Item::Alias {
            doc: Vec::new(),
            export: true,
            name: "schemas".into(),
            ty: Type::Index(Box::new(Type::name("components")), "schemas".into()),
        });
        items.push(Item::Blank);
        self.definitions(&mut items)?;
        self.exports(&mut items);
        items.push(Item::Alias {
            doc: vec!["Security schemes declared in the document.".into()],
            export: true,
            name: "SecuritySchemeName".into(),
            ty: Type::Union(
                api.security_schemes
                    .iter()
                    .map(|s| Type::string(&s.key))
                    .collect(),
            ),
        });
        items.push(Item::Blank);

        items.push(Item::Raw(RUNTIME.into()));
        items.push(Item::Blank);
        items.push(self.spec(methods, data));
        Ok(ast::print(&items))
    }

    /// Each component schema under its own name, and the values of each enum.
    /// The names are exported from the `$` aliases rather than declared, so they
    /// never shadow a global the runtime uses (`Response`, `Record`, ...).
    fn exports(&self, items: &mut Vec<Item>) {
        let mut taken: HashSet<String> = RESERVED.iter().map(|s| s.to_string()).collect();
        taken.extend(runtime_exports());
        let components: Vec<&NamedSchema> =
            self.api.schemas.iter().filter(|s| !s.hoisted).collect();
        // (alias, exported name); request shapes come after every schema's own
        // name, so adding one never renames another schema.
        let mut types = Vec::new();
        let mut enums = Vec::new();
        for s in &components {
            let local = self.reference(&s.key, View::Response);
            let name = unique(export_name(&s.key), &mut taken);
            match &s.schema {
                Schema::Enum { values, .. } => enums.push((local, name, values)),
                _ => types.push((local, name)),
            }
        }
        for s in &components {
            if self.request_variants.contains(s.key.as_str()) {
                let name = unique(format!("{}Request", export_name(&s.key)), &mut taken);
                types.push((self.reference(&s.key, View::Request), name));
            }
        }
        if !types.is_empty() {
            items.push(Item::Comment(vec![
                "Each component schema under its own name; `<name>Request` is its request shape."
                    .into(),
            ]));
            items.push(Item::ExportTypes(types));
            items.push(Item::Blank);
        }
        if !enums.is_empty() {
            items.push(Item::Comment(vec![
                "The values of each enum schema, exported with its type.".into(),
            ]));
            let mut specifiers = Vec::new();
            for (local, name, values) in enums {
                let values = values.iter().cloned().map(Expr::Literal).collect();
                items.push(Item::Const {
                    name: local.clone(),
                    ty: None,
                    value: Expr::Array(values),
                });
                specifiers.push((local, name));
            }
            items.push(Item::Export(specifiers));
            items.push(Item::Blank);
        }
    }

    /// One `components` member: the component schemas, in one view.
    fn schema_group(&self, name: &str, view: View) -> Member {
        let schemas = self
            .api
            .schemas
            .iter()
            .filter(|s| !s.hoisted)
            .filter(|s| view == View::Response || self.request_variants.contains(s.key.as_str()))
            .map(|s| Member::new(&s.key, false, Type::name(self.reference(&s.key, view))))
            .collect();
        Member::new(name, false, Type::object(schemas))
    }

    /// One type alias per schema, and one for its request view where that differs.
    fn definitions(&self, items: &mut Vec<Item>) -> Result<(), Error> {
        if self.api.schemas.is_empty() {
            return Ok(());
        }
        items.push(Item::Comment(vec![
            "One type alias per schema, `$` plus its name; `$<name>$Request` is the request".into(),
            "shape where readOnly or writeOnly properties make it differ.".into(),
        ]));
        let mut edges = Vec::new();
        for s in &self.api.schemas {
            let key = s.key.as_str();
            items.push(Item::Alias {
                doc: doc(&s.meta, &[]),
                export: false,
                name: self.reference(key, View::Response),
                ty: match self.define(s, View::Response, &mut edges) {
                    Type::Intersection(ts) if ts.is_empty() => {
                        return Err(Error::new(
                            "the schema has no type of its own: it only extends schemas that list it in oneOf or anyOf; give it or a schema it extends a `type` or `properties`",
                        )
                        .about_origin(s.origin)
                        .context(s.location()));
                    }
                    ty => ty,
                },
            });
            if self.request_variants.contains(key) {
                items.push(Item::Alias {
                    doc: doc(&s.meta, &[]),
                    export: false,
                    name: self.reference(key, View::Request),
                    ty: self.define(s, View::Request, &mut Vec::new()),
                });
            }
        }
        items.push(Item::Blank);
        check_cycles(self.api, &edges)
    }

    /// Runtime data: default base URL, the document's methods and security, and
    /// what each operation needs beyond the defaults (`operations`, as `"METHOD
    /// /path"` and data).
    fn spec(&self, methods: Vec<&str>, operations: Vec<(String, Expr)>) -> Item {
        let api = self.api;
        let base_url = api
            .servers
            .first()
            .map(Server::default_url)
            .unwrap_or_default();
        let schemes = api
            .security_schemes
            .iter()
            .map(|s| {
                let mut spec = Vec::new();
                let mut field = |k: &str, v: &str| spec.push((k.to_string(), Expr::string(v)));
                match &s.kind {
                    SecuritySchemeKind::ApiKey { name, location } => {
                        field("type", "apiKey");
                        field(
                            "in",
                            match location {
                                ApiKeyLocation::Query => "query",
                                ApiKeyLocation::Header => "header",
                                ApiKeyLocation::Cookie => "cookie",
                            },
                        );
                        field("name", name);
                    }
                    SecuritySchemeKind::Http { scheme } => {
                        field("type", "http");
                        field("scheme", scheme);
                    }
                    SecuritySchemeKind::MutualTls => field("type", "mutualTLS"),
                    SecuritySchemeKind::OAuth2 => field("type", "oauth2"),
                    SecuritySchemeKind::OpenIdConnect { .. } => field("type", "openIdConnect"),
                }
                (s.key.clone(), Expr::inline(spec))
            })
            .collect();
        Item::Const {
            name: "spec".into(),
            ty: Some(Type::name("Spec")),
            value: Expr::object(vec![
                ("baseUrl".into(), Expr::string(&base_url)),
                (
                    "methods".into(),
                    Expr::Array(methods.into_iter().map(Expr::string).collect()),
                ),
                ("security".into(), security_literal(&api.security)),
                ("securitySchemes".into(), Expr::object(schemes)),
                ("operations".into(), Expr::object(operations)),
            ]),
        }
    }

    /// The runtime data of `op`, keyed by `"METHOD /path"`, if it needs any.
    /// `binary` is [`Emitter::binary_text`] of each response of `op`, and
    /// `content_type` its default request media type.
    fn operation_data(
        &self,
        item: &PathItem,
        op: &Operation,
        binary: &[Vec<String>],
        content_type: Option<&str>,
    ) -> Option<(String, Expr)> {
        let mut info = Vec::new();
        if let Some(id) = &op.operation_id {
            info.push(("operationId".into(), Expr::string(id)));
        }
        if let Some(s) = op.servers.first().or(item.servers.first()) {
            info.push(("server".into(), Expr::string(&s.default_url())));
        }
        let styles: Vec<(String, Expr)> = op
            .params
            .iter()
            .filter_map(|p| {
                let name = format!("{}:{}", p.location.as_str(), p.name);
                Some((name, param_style(p)?))
            })
            .collect();
        if !styles.is_empty() {
            info.push(("styles".into(), Expr::inline(styles)));
        }
        if let Some(security) = &op.security {
            info.push(("security".into(), security_literal(security)));
        }
        if let Some(qs) = &op.querystring {
            info.push(("querystring".into(), Expr::string(&qs.content.media_type)));
            if let Some(e) = self.encoding_literal(&qs.content) {
                info.push(("querystringEncoding".into(), e));
            }
        }
        if let Some(ct) = content_type {
            info.push(("contentType".into(), Expr::string(ct)));
        }
        let encodings: Vec<(String, Expr)> = op
            .body
            .iter()
            .flat_map(|b| &b.content)
            .filter_map(|c| Some((c.media_type.clone(), self.encoding_literal(c)?)))
            .collect();
        if !encodings.is_empty() {
            info.push(("encoding".into(), Expr::inline(encodings)));
        }
        // If any response has binary text, include every response key so the
        // runtime finds the key covering a status the same way the types do.
        if binary.iter().any(|b| !b.is_empty()) {
            let keys = op.responses.iter().zip(binary).map(|(r, types)| {
                let types = types.iter().map(|mt| Expr::string(mt)).collect();
                (r.status.key(), Expr::Array(types))
            });
            info.push(("binary".into(), Expr::inline(keys.collect())));
        }
        (!info.is_empty()).then(|| (format!("{} {}", op.method, item.path), Expr::inline(info)))
    }

    /// One `paths` member, and the runtime data of the operation (see
    /// [`Emitter::operation_data`]); its `statuses` alias goes to `statuses`
    /// (name -> type).
    fn operation(
        &self,
        item: &PathItem,
        op: &Operation,
        statuses: &mut BTreeMap<String, Type>,
    ) -> (Member, Option<(String, Expr)>) {
        let mut extra = Vec::new();
        if let Some(id) = &op.operation_id {
            extra.push(format!("operationId: {id}"));
        }
        let mut responses = Vec::new();
        if op.responses.is_empty() {
            responses.push(Member::new("default", false, Type::name("unknown")));
        }
        let binary: Vec<Vec<String>> = op.responses.iter().map(|r| self.binary_text(r)).collect();
        let content_type = op.body.as_ref().and_then(Body::default_media_type);
        for (r, binary) in op.responses.iter().zip(&binary) {
            // Never carries a body on the wire, whatever the document declares.
            let bodiless = op.method == "HEAD" || matches!(r.status, Status::Code(204 | 205 | 304));
            let data = if bodiless || r.content.is_empty() {
                Type::name("undefined")
            } else {
                Type::Union(
                    r.content
                        .iter()
                        .map(|c| self.response_data(c, binary))
                        .collect(),
                )
            };
            responses.push(Member::new(r.status.key(), false, data).doc(doc(&r.meta, &[])));
        }
        let (name, ty) = status_alias(op);
        let members = vec![
            Member::new("request", false, self.request(op, content_type)),
            Member::new("responses", false, Type::object(responses)),
            Member::new("statuses", false, Type::name(&name)),
        ];
        statuses.entry(name).or_insert(ty);
        let member =
            Member::new(&op.method, false, Type::object(members)).doc(doc(&op.meta, &extra));
        (member, self.operation_data(item, op, &binary, content_type))
    }

    /// `default` is the media type implied when `contentType` is left out.
    fn request(&self, op: &Operation, default: Option<&str>) -> Type {
        let mut fields = Vec::new();
        for loc in [
            Location::Path,
            Location::Query,
            Location::Header,
            Location::Cookie,
        ] {
            let params: Vec<&Param> = op.params.iter().filter(|p| p.location == loc).collect();
            if params.is_empty() {
                continue;
            }
            let required = params.iter().any(|p| p.required);
            let members = params
                .iter()
                .map(|p| {
                    let t = match &p.value {
                        ParamValue::Schema { schema, .. } => self.ty(schema, View::Request),
                        ParamValue::Content(c)
                            if loc == Location::Query && c.kind() == MediaKind::Json =>
                        {
                            self.schema_or_unknown(c.schema.as_ref(), View::Request)
                        }
                        // Other content-based parameters are passed pre-serialized.
                        ParamValue::Content(_) => Type::name("string"),
                    };
                    property(&p.name, !p.required, t).doc(doc(&p.meta, &[]))
                })
                .collect();
            fields.push(Member::new(loc.as_str(), !required, Type::object(members)));
        }
        if let Some(qs) = &op.querystring {
            let t = match (qs.content.kind(), &qs.content.schema) {
                (MediaKind::FormUrlEncoded, Some(s)) if *s != Schema::Any => {
                    self.ty(s, View::Request)
                }
                (MediaKind::FormUrlEncoded, _) => {
                    Type::Generic("Record", vec![Type::name("string"), Type::name("unknown")])
                }
                (MediaKind::Json, s) => self.schema_or_unknown(s.as_ref(), View::Request),
                _ => Type::name("string"),
            };
            fields.push(Member::new("querystring", !qs.required, t));
        }

        // One set of fields per request media type; several become a union.
        let mut variants: Vec<Vec<Member>> = Vec::new();
        if let Some(body) = &op.body {
            for c in &body.content {
                let range = essence(&c.media_type);
                let ct = if range == "*/*" {
                    // Without a default media type, leaving it out sends the body by
                    // its kind (JSON, or bytes as they are), which `*/*` covers.
                    Member::new("contentType", default.is_none(), Type::name("string"))
                } else if range.contains('*') {
                    // A media type in the range, such as `image/png` for `image/*`.
                    let parts = range.split('*').map(String::from).collect();
                    Member::new("contentType", false, Type::Template(parts))
                } else {
                    let implied = default == Some(c.media_type.as_str());
                    Member::new("contentType", implied, Type::string(&c.media_type))
                };
                let body_field = Member::new("body", !body.required, self.request_body(c))
                    .doc(doc(&body.meta, &[]));
                variants.push(vec![ct, body_field]);
            }
            // An optional body can be left out without naming a media type,
            // even when no media type is implied.
            let implied =
                default.is_some() || body.content.iter().any(|c| essence(&c.media_type) == "*/*");
            if !body.required && !implied && !variants.is_empty() {
                variants.push(vec![
                    Member::new("contentType", true, Type::name("never")),
                    Member::new("body", true, Type::name("never")),
                ]);
            }
        }

        match variants.len() {
            0 => Type::object(fields),
            1 => {
                fields.append(&mut variants[0]);
                Type::object(fields)
            }
            _ => {
                let bodies = Type::Union(variants.into_iter().map(Type::object).collect());
                if fields.is_empty() {
                    bodies
                } else {
                    Type::Intersection(vec![Type::object(fields), bodies])
                }
            }
        }
    }

    fn request_body(&self, c: &Content) -> Type {
        match c.kind() {
            MediaKind::Json | MediaKind::FormUrlEncoded | MediaKind::Unknown => {
                self.schema_or_unknown(c.schema.as_ref(), View::Request)
            }
            // `FormData` is always sent as `multipart/form-data`. With a
            // `Blob`, its `type` (media type and boundary) becomes
            // `Content-Type`.
            MediaKind::Multipart if essence(&c.media_type) != "multipart/form-data" => {
                Type::name("Blob")
            }
            MediaKind::Multipart => match &c.schema {
                Some(s) if *self.schemas.resolve(s) != Schema::Any => {
                    self.form_data(s, &mut Vec::new())
                }
                _ => Type::name("FormData"),
            },
            MediaKind::Text => Type::name("string"),
            // Sent as-is. Fetch encodes these two in their own formats.
            _ => Type::Generic(
                "Exclude",
                vec![
                    Type::name("BodyInit"),
                    Type::Union(vec![Type::name("FormData"), Type::name("URLSearchParams")]),
                ],
            ),
        }
    }

    /// A `multipart/form-data` body of schema `s`, whose properties are typed
    /// by [`Emitter::part`]. `$ref`s are followed through `allOf`, `oneOf` and
    /// `anyOf`, so that the properties of the schemas they name are parts too.
    fn form_data<'s>(&'s self, s: &'s Schema, visiting: &mut Vec<&'s str>) -> Type {
        let view = View::Request;
        match s {
            Schema::Ref(k) if !visiting.contains(&k.as_str()) => {
                visiting.push(k);
                let t = self.form_data(&self.schemas[k.as_str()].schema, visiting);
                visiting.pop();
                t
            }
            Schema::Object {
                properties,
                pattern_properties,
                additional,
            } => self.object(
                properties,
                pattern_properties,
                additional.as_deref(),
                view,
                &|s| self.part(s),
            ),
            Schema::Union {
                variants,
                discriminator,
                ..
            } => Type::Union(
                variants
                    .iter()
                    .map(|v| {
                        let t = self.tag(self.form_data(v, visiting), v, discriminator.as_ref());
                        exclusive(t, v, s)
                    })
                    .collect(),
            ),
            Schema::Intersection(parts) => {
                let hidden = self.hidden_in(parts, view);
                Type::Intersection(
                    parts
                        .iter()
                        .map(|p| {
                            // A bare `type: object` is `object` here.
                            let t = if is_bare_object(p) {
                                Type::name("object")
                            } else {
                                self.form_data(p, visiting)
                            };
                            self.omit(t, p, &hidden, view, None)
                        })
                        .collect(),
                )
            }
            _ => self.ty(s, view),
        }
    }

    /// Types a property of a `multipart/form-data` body. A schema with no
    /// `type` (how binary data is described) becomes `Blob`, and an array of
    /// such items becomes an array of `Blob`s, sent as parts with the same
    /// name.
    fn part(&self, s: &Schema) -> Type {
        match self.schemas.resolve(s) {
            Schema::Any => Type::name("Blob"),
            Schema::Array {
                prefix_items,
                items,
                min_items,
            } if prefix_items.is_empty()
                && items
                    .as_deref()
                    .is_none_or(|i| *self.schemas.resolve(i) == Schema::Any) =>
            {
                array(Type::name("Blob"), *min_items)
            }
            _ => self.ty(s, View::Request),
        }
    }

    /// Builds the runtime `Encoding` map for form-body properties that need
    /// one: properties given an Encoding Object in the document, and
    /// `multipart` parts for encoded binary strings, which are not text.
    fn encoding_literal(&self, c: &Content) -> Option<Expr> {
        let multipart = essence(&c.media_type) == "multipart/form-data";
        let mut fields: Vec<(String, Expr)> = c
            .encoding
            .iter()
            .filter_map(|e| {
                let name = &e.name;
                let e = match e.serialization.as_ref()? {
                    // A list or range of media types leaves the choice of a
                    // field's media type to its value.
                    Serialization::ContentType(ct) if ct.contains([',', '*']) => {
                        return None;
                    }
                    Serialization::ContentType(ct) => ("contentType".into(), Expr::string(ct)),
                    Serialization::Style {
                        style,
                        explode,
                        allow_reserved,
                    } => (
                        "style".into(),
                        // `multipart` parts are not percent-encoded at all.
                        style_literal(*style, *explode, *allow_reserved && !multipart)
                            .unwrap_or(Expr::inline(Vec::new())),
                    ),
                };
                Some((name.clone(), Expr::inline(vec![e])))
            })
            .collect();
        if multipart && let Some(s) = &c.schema {
            let mut named: HashSet<&str> = c
                .encoding
                .iter()
                .filter(|e| e.serialization.is_some())
                .map(|e| e.name.as_str())
                .collect();
            let mut properties = Vec::new();
            self.parts(s, &mut Vec::new(), &mut properties);
            for p in properties {
                let encoded = |s: &Schema| {
                    matches!(
                        self.schemas.resolve(s),
                        Schema::String {
                            content_encoding: Some(_),
                            ..
                        }
                    )
                };
                let binary = match self.schemas.resolve(&p.schema) {
                    Schema::Array { items: Some(i), .. } => encoded(i),
                    s => encoded(s),
                };
                if binary && named.insert(&p.name) {
                    let octets = Expr::string("application/octet-stream");
                    fields.push((
                        p.name.clone(),
                        Expr::inline(vec![("contentType".into(), octets)]),
                    ));
                }
            }
        }
        (!fields.is_empty()).then(|| Expr::inline(fields))
    }

    /// The properties of `multipart/form-data` body schema `s`, through the
    /// schemas that [`Emitter::form_data`] follows.
    fn parts<'s>(
        &'s self,
        s: &'s Schema,
        visiting: &mut Vec<&'s str>,
        out: &mut Vec<&'s Property>,
    ) {
        match s {
            Schema::Ref(k) if !visiting.contains(&k.as_str()) => {
                visiting.push(k);
                self.parts(&self.schemas[k.as_str()].schema, visiting, out);
                visiting.pop();
            }
            Schema::Object { properties, .. } => out.extend(properties),
            Schema::Union { variants: ss, .. } | Schema::Intersection(ss) => {
                for s in ss {
                    self.parts(s, visiting, out);
                }
            }
            _ => {}
        }
    }

    /// The `text/*` media types of response `r` whose schema marks the body as
    /// binary (no `type`, or `format: binary` in 3.0), as essences. The runtime
    /// uses the response key covering the status and the actual `Content-Type`
    /// to read matching text responses as a `Blob`.
    fn binary_text(&self, r: &Response) -> Vec<String> {
        let mut found = Vec::new();
        for c in &r.content {
            let binary = c.kind() == MediaKind::Text
                && c.schema
                    .as_ref()
                    .is_some_and(|s| *self.schemas.resolve(s) == Schema::Any);
            let mt = essence(&c.media_type);
            if binary && !found.contains(&mt) {
                found.push(mt);
            }
        }
        found
    }

    /// `binary` is [`Emitter::binary_text`] of the response.
    fn response_data(&self, c: &Content, binary: &[String]) -> Type {
        let kind = c.kind();
        if kind.is_sequential() {
            let item = match (kind, c.sequence_item()) {
                (_, Some(s)) => self.ty(s, View::Response),
                (MediaKind::EventStream, None) => Type::name("ServerSentEvent"),
                _ => Type::name("unknown"),
            };
            return Type::Generic("AsyncIterable", vec![item]);
        }
        match kind {
            MediaKind::Json => self.schema_or_unknown(c.schema.as_ref(), View::Response),
            MediaKind::Text if covers(binary, &essence(&c.media_type)) => Type::name("Blob"),
            // `text/*` also covers event streams and `+json` types, which the
            // runtime decodes differently. The actual Content-Type decides.
            MediaKind::Text if c.media_type.contains('*') => Type::name("unknown"),
            MediaKind::Text | MediaKind::FormUrlEncoded => Type::name("string"),
            // A range such as `*/*` is decoded by the actual Content-Type.
            MediaKind::Unknown => Type::name("unknown"),
            _ => Type::name("Blob"),
        }
    }

    fn schema_or_unknown(&self, s: Option<&Schema>, view: View) -> Type {
        s.map(|s| self.ty(s, view))
            .unwrap_or_else(|| Type::name("unknown"))
    }

    /// The alias of schema `k` in `view`.
    fn reference(&self, k: &str, view: View) -> String {
        let name = &self.names[k];
        if view == View::Request && self.request_variants.contains(k) {
            format!("{name}$Request")
        } else {
            name.clone()
        }
    }

    /// The TypeScript type of a schema.
    fn ty(&self, s: &Schema, view: View) -> Type {
        match s {
            Schema::Any => Type::name("unknown"),
            Schema::Never => Type::name("never"),
            Schema::Ref(k) => Type::name(self.reference(k, view)),
            Schema::Null => Type::name("null"),
            Schema::Boolean => Type::name("boolean"),
            Schema::Integer { .. } | Schema::Number { .. } => Type::name("number"),
            Schema::String { .. } => Type::name("string"),
            Schema::Const { value, .. } => Type::Literal(value.clone()),
            Schema::Enum { values, .. } => {
                Type::Union(values.iter().cloned().map(Type::Literal).collect())
            }
            Schema::Array {
                prefix_items,
                items,
                min_items,
            } => {
                let unknown = || Type::name("unknown");
                if prefix_items.is_empty() {
                    let item = items.as_deref().map_or_else(unknown, |s| self.ty(s, view));
                    return array(item, *min_items);
                }
                let rest = match items.as_deref() {
                    Some(Schema::Never) => None,
                    Some(rest) => Some(self.ty(rest, view)),
                    None => Some(unknown()),
                };
                Type::Tuple(
                    prefix_items.iter().map(|s| self.ty(s, view)).collect(),
                    rest.map(Box::new),
                )
            }
            Schema::Object {
                properties,
                pattern_properties,
                additional,
            } => self.object(
                properties,
                pattern_properties,
                additional.as_deref(),
                view,
                &|s| self.ty(s, view),
            ),
            Schema::Union {
                variants,
                discriminator,
                ..
            } => Type::Union(
                variants
                    .iter()
                    .map(|v| {
                        let t = self.tag(self.ty(v, view), v, discriminator.as_ref());
                        exclusive(t, v, s)
                    })
                    .collect(),
            ),
            Schema::Intersection(parts) => {
                let by_unions = self.hidden_by_unions(parts, view);
                let mut hidden = self.hidden_in(parts, view);
                hidden.extend(by_unions.iter().cloned());
                Type::Intersection(
                    parts
                        .iter()
                        .map(|p| {
                            let t = self.composed(p, view);
                            match p {
                                Schema::Object { .. } => self.strip(t, p, &by_unions, view, None),
                                _ => self.omit(t, p, &hidden, view, None),
                            }
                        })
                        .collect(),
                )
            }
        }
    }

    /// The properties that every member of a `oneOf` or `anyOf` among `parts`
    /// leaves out of `view`. A value has to match one of the members, so it
    /// cannot have such a property, even if another part requires it: a
    /// parent that lists its subtypes may require what each of them marks
    /// `readOnly`.
    fn hidden_by_unions(&self, parts: &[Schema], view: View) -> BTreeSet<String> {
        let mut hidden = BTreeSet::new();
        for p in parts {
            let Schema::Union { variants, .. } = p else {
                continue;
            };
            let mut sets = variants
                .iter()
                .map(|v| self.hidden_in(std::slice::from_ref(v), view));
            if let Some(first) = sets.next() {
                hidden.extend(sets.fold(first, |all, set| &all & &set));
            }
        }
        hidden
    }

    /// The properties of `parts`, an `allOf`, that are absent from `view`:
    /// `readOnly` ones from requests, `writeOnly` ones from responses.
    fn hidden_in(&self, parts: &[Schema], view: View) -> BTreeSet<String> {
        let (read_only, write_only) = self.schemas.flagged_in(parts);
        match view {
            View::Request => read_only,
            View::Response => write_only,
        }
    }

    /// Wraps `t` (the type of `allOf` member `s`) in `Omit` to strip any
    /// properties in `hidden` that `s` exposes in `view`, when `s` is a `$ref`
    /// or a nested `allOf` that may contain `$ref`s. Another member marks those
    /// properties `readOnly` or `writeOnly` (`?: never` in this view), while a
    /// referenced schema is shared and may still require them. Without `Omit`,
    /// `number & never` would reject every value. `t` is left unchanged if `s`
    /// contains a union, since `Omit` would merge its members, or an object
    /// with an index signature, since `Omit` would drop all named properties
    /// and leave only the index signature.
    ///
    /// When `s` is a parent expanded in place of a reference (see
    /// [`Emitter::eager`]), `child` names the schema being defined. Unions that
    /// list `child` are left out of `t`, so they are ignored here too.
    fn omit(
        &self,
        t: Type,
        s: &Schema,
        hidden: &BTreeSet<String>,
        view: View,
        child: Option<&str>,
    ) -> Type {
        if !matches!(s, Schema::Ref(_) | Schema::Intersection(_)) {
            return t;
        }
        self.strip(t, s, hidden, view, child)
    }

    /// [`Emitter::omit`] for any `s`, including an object written in place.
    fn strip(
        &self,
        t: Type,
        s: &Schema,
        hidden: &BTreeSet<String>,
        view: View,
        child: Option<&str>,
    ) -> Type {
        if hidden.is_empty() {
            return t;
        }
        let Some(declared) = self.declared(s, view, child) else {
            return t;
        };
        let names: Vec<Type> = hidden
            .iter()
            .filter(|n| declared.contains(n.as_str()))
            .map(|n| Type::string(n))
            .collect();
        if names.is_empty() {
            t
        } else {
            Type::Generic("Omit", vec![t, Type::Union(names)])
        }
    }

    /// Returns the property names `s` declares and shows in `view` through
    /// `$ref` and `allOf`, or `None` if `s` is or contains an object with an
    /// index signature or a union other than one that lists `child`.
    fn declared<'s>(
        &'s self,
        s: &'s Schema,
        view: View,
        child: Option<&str>,
    ) -> Option<HashSet<&'s str>> {
        let mut names = HashSet::new();
        for s in self.schemas.flatten(s) {
            match s {
                Schema::Object {
                    pattern_properties,
                    additional,
                    ..
                } if !pattern_properties.is_empty()
                    || additional.as_deref().is_some_and(|a| *a != Schema::Never) =>
                {
                    return None;
                }
                Schema::Object { properties, .. } => {
                    let shown = properties.iter().filter(|p| match view {
                        View::Request => !p.meta.read_only,
                        View::Response => !p.meta.write_only,
                    });
                    names.extend(shown.map(|p| p.name.as_str()));
                }
                Schema::Union { .. } if child.is_some_and(|c| self.reaches(s, c)) => {}
                Schema::Union { .. } => return None,
                _ => {}
            }
        }
        Some(names)
    }

    /// The type of an intersection member. In an intersection, `type: object`
    /// without other keywords only says the value is an object. As an index
    /// signature, it would accept any property name the other members leave
    /// out.
    fn composed(&self, s: &Schema, view: View) -> Type {
        match s {
            _ if is_bare_object(s) => Type::name("object"),
            Schema::Union {
                kind: UnionKind::Types,
                variants,
                ..
            } => Type::Union(variants.iter().map(|v| self.composed(v, view)).collect()),
            _ => self.ty(s, view),
        }
    }

    /// An object type whose property, pattern and additional values are typed
    /// by `ty`.
    fn object(
        &self,
        properties: &[Property],
        pattern_properties: &[(String, Schema)],
        additional: Option<&Schema>,
        view: View,
        ty: &dyn Fn(&Schema) -> Type,
    ) -> Type {
        let unknown = || Some(Type::name("unknown"));
        let index = match (properties.is_empty(), additional) {
            // TypeScript cannot restrict keys to a pattern, so the index
            // signature covers all keys with the union of the pattern
            // types and, unless disallowed, `additionalProperties`.
            (true, Some(s)) if !pattern_properties.is_empty() => {
                let mut types: Vec<Type> = pattern_properties.iter().map(|(_, p)| ty(p)).collect();
                if *s != Schema::Never {
                    types.push(ty(s));
                }
                Some(Type::Union(types))
            }
            (false, _) if !pattern_properties.is_empty() => unknown(),
            (_, Some(Schema::Never)) => None,
            (true, Some(s)) => Some(ty(s)),
            (true, None) => unknown(),
            // Index signatures must cover every declared property type.
            (false, Some(_)) => unknown(),
            (false, None) => None,
        };
        if properties.is_empty() {
            return match index {
                Some(t) => Type::inline(vec![Member::index(t)]),
                None => Type::Generic("Record", vec![Type::name("string"), Type::name("never")]),
            };
        }
        let mut members: Vec<Member> = properties
            .iter()
            .map(|p| {
                // A readOnly property should not be sent in a request, nor a
                // writeOnly one in a response.
                let absent = (view == View::Request && p.meta.read_only)
                    || (view == View::Response && p.meta.write_only);
                // A required property can be anything but undefined; this also makes
                // `{ id: {} | null } & Base` narrow `Base["id"]` instead of widening it.
                let t = match ty(&p.schema) {
                    _ if absent => Type::name("never"),
                    t if t.is("unknown") && p.required => {
                        Type::Union(vec![Type::inline(Vec::new()), Type::name("null")])
                    }
                    t => t,
                };
                property(&p.name, !p.required || absent, t).doc(doc(&p.meta, &[]))
            })
            .collect();
        members.extend(index.map(Member::index));
        Type::object(members)
    }

    /// Union variant `v` as type `t`, tagged with its discriminating values when
    /// it is a reference, so that the union narrows on them.
    fn tag(&self, t: Type, v: &Schema, d: Option<&Discriminator>) -> Type {
        let (Some(d), Schema::Ref(k)) = (d, v) else {
            return t;
        };
        // Mapping keys are always strings, but the values they stand for have
        // the property's type.
        let typed = self
            .schemas
            .property(v, &d.property_name)
            .filter(|p| !self.schemas.is_text(p));
        // The default variant matches unmapped property values as well as
        // objects without the property; the variant's schema decides whether
        // the property is required.
        let (optional, values) = if d.default_mapping.as_deref() == Some(k.as_str()) {
            if typed.is_some() {
                return t;
            }
            (true, Type::name("string"))
        } else {
            let values: Vec<Type> = d
                .mapping
                .iter()
                .filter(|(_, target)| target == k)
                .filter_map(|(value, _)| match typed {
                    Some(p) => self.schemas.mapped_value(p, value).map(Type::Literal),
                    None => Some(Type::string(value)),
                })
                .collect();
            if values.is_empty() {
                return t;
            }
            (false, Type::Union(values))
        };
        let tag = property(&d.property_name, optional, values);
        Type::Intersection(vec![t, Type::inline(vec![tag])])
    }

    /// Builds the type for a named schema's alias.
    ///
    /// TypeScript resolves the top level of a type alias eagerly, so a cycle
    /// through `$ref`, `allOf`, `oneOf` and `anyOf` alone is an error (TS2456),
    /// whereas a cycle through an object or array is fine. A parent that lists
    /// its subtypes in `oneOf` while each subtype extends it with `allOf` (as
    /// springdoc emits) forms such a cycle. The cycle is broken on the
    /// subtype's side by replacing the reference to the parent with the
    /// parent's own part (its type without the subtype list). If neither has a
    /// part of its own, the result is an empty intersection, which the caller
    /// rejects. Remaining references in eager positions are recorded in `edges`
    /// for [`check_cycles`].
    fn define(&self, s: &'a NamedSchema, view: View, edges: &mut Vec<(&'a str, &'a str)>) -> Type {
        let mut d = Defining {
            key: &s.key,
            view,
            expanding: Vec::new(),
            edges: Vec::new(),
        };
        let parts = self.eager(&s.schema, &mut d, true);
        edges.append(&mut d.edges);
        Type::Intersection(parts)
    }

    /// Collects the top-level intersection members of `s` for the alias of
    /// `d.key`. `member` is true when `s` is the whole alias or an `allOf`
    /// member; there, a reference to a parent that lists `d.key` is replaced
    /// with the parent's own part, and while expanding that parent, any
    /// union that lists `d.key` is dropped.
    fn eager(&self, s: &'a Schema, d: &mut Defining<'a>, member: bool) -> Vec<Type> {
        let view = d.view;
        match s {
            Schema::Ref(k) if member && self.enumerates(k, d.key) => {
                if d.expanding.contains(&k.as_str()) {
                    return Vec::new();
                }
                d.expanding.push(k);
                let parts = self.eager(&self.schemas[k.as_str()].schema, d, true);
                d.expanding.pop();
                parts
            }
            Schema::Ref(k) => {
                d.edges.push((d.key, k));
                vec![Type::name(self.reference(k, view))]
            }
            Schema::Union {
                variants,
                discriminator,
                ..
            } => {
                if !d.expanding.is_empty() && self.reaches(s, d.key) {
                    return Vec::new();
                }
                let types = variants
                    .iter()
                    .map(|v| {
                        let t = Type::Intersection(self.eager(v, d, false));
                        exclusive(self.tag(t, v, discriminator.as_ref()), v, s)
                    })
                    .collect();
                vec![Type::Union(types)]
            }
            Schema::Intersection(parts) => {
                // While a parent is expanded in place, its unions that list
                // this schema are dropped, so they hide nothing.
                let by_unions = if d.expanding.is_empty() {
                    self.hidden_by_unions(parts, view)
                } else {
                    BTreeSet::new()
                };
                let mut hidden = self.hidden_in(parts, view);
                hidden.extend(by_unions.iter().cloned());
                let mut types = Vec::new();
                for p in parts {
                    match p {
                        Schema::Object { .. } => {
                            let t = self.composed(p, view);
                            types.push(self.strip(t, p, &by_unions, view, None));
                        }
                        Schema::Union {
                            kind: UnionKind::Types,
                            ..
                        } => types.push(self.composed(p, view)),
                        // Expanding a parent in place drops unions that
                        // list this schema, so they should not stop `omit`
                        // from stripping hidden properties.
                        _ => {
                            let ts = self.eager(p, d, true);
                            if ts.is_empty() {
                                continue;
                            }
                            match self.omit(Type::Intersection(ts), p, &hidden, view, Some(d.key)) {
                                Type::Intersection(ts) => types.extend(ts),
                                t => types.push(t),
                            }
                        }
                    }
                }
                types
            }
            _ => vec![self.ty(s, view)],
        }
    }

    /// Whether schema `p` lists `c` among its subtypes: `c` is reachable from
    /// it through `$ref`, `allOf`, `oneOf` and `anyOf` alone, crossing at least
    /// one `oneOf` or `anyOf`.
    fn enumerates(&self, p: &str, c: &str) -> bool {
        self.reaches(&self.schemas[p].schema, c)
    }

    /// Whether `c` is reachable from `s` through `$ref`, `allOf`, `oneOf` and
    /// `anyOf` alone, crossing a `oneOf` or `anyOf`.
    fn reaches<'s>(&'s self, s: &'s Schema, c: &str) -> bool {
        let mut stack = vec![(s, false)];
        let mut seen = HashSet::new();
        while let Some((s, crossed)) = stack.pop() {
            match s {
                Schema::Ref(k) if k.as_str() == c => {
                    if crossed {
                        return true;
                    }
                }
                Schema::Ref(k) => {
                    if seen.insert((k.as_str(), crossed)) {
                        stack.push((&self.schemas[k.as_str()].schema, crossed));
                    }
                }
                Schema::Union { variants, .. } => {
                    stack.extend(variants.iter().map(|v| (v, true)));
                }
                Schema::Intersection(parts) => {
                    stack.extend(parts.iter().map(|p| (p, crossed)));
                }
                _ => {}
            }
        }
        false
    }
}

/// State while rendering one schema's alias (see [`Emitter::define`]).
struct Defining<'a> {
    /// The schema being defined.
    key: &'a str,
    view: View,
    /// Parents whose own part is being rendered in place of a reference.
    expanding: Vec<&'a str>,
    /// References left in eager positions, as (from, to).
    edges: Vec<(&'a str, &'a str)>,
}

/// Rejects a cycle among the references TypeScript resolves eagerly (see
/// [`Emitter::define`]), which would make every alias on it `any` (TS2456).
fn check_cycles(api: &Api, edges: &[(&str, &str)]) -> Result<(), Error> {
    let mut targets: HashMap<&str, Vec<&str>> = HashMap::new();
    for (from, to) in edges {
        targets.entry(from).or_default().push(to);
    }
    // Depth-first search for a back edge, in document order.
    let mut state: HashMap<&str, u8> = HashMap::new(); // 1: on the path, 2: done
    for start in api.schemas.iter().map(|s| s.key.as_str()) {
        if state.contains_key(start) {
            continue;
        }
        state.insert(start, 1);
        let mut path = vec![(start, 0)];
        while let Some(&(node, next)) = path.last() {
            let Some(&to) = targets.get(node).and_then(|ts| ts.get(next)) else {
                state.insert(node, 2);
                path.pop();
                continue;
            };
            path.last_mut().unwrap().1 += 1;
            match state.get(to) {
                None => {
                    state.insert(to, 1);
                    path.push((to, 0));
                }
                Some(1) => {
                    let from = path.iter().position(|(n, _)| *n == to).unwrap();
                    let cycle: Vec<&str> =
                        path[from..].iter().map(|(n, _)| *n).chain([to]).collect();
                    let schema = api.schemas.iter().find(|s| s.key == to).unwrap();
                    return Err(Error::new(format!(
                        "a schema cannot reference itself through $ref, allOf, oneOf or anyOf alone: {}",
                        cycle.join(" -> ")
                    ))
                    .about_origin(schema.origin)
                    .context(schema.location()));
                }
                Some(_) => {}
            }
        }
    }
    Ok(())
}

/// Whether `s` is `type: object` without other keywords.
/// The property names of a schema that only requires properties, such as
/// `required: [a, b]`: it matches exactly the objects that have them all.
fn required_only(s: &Schema) -> Option<Vec<&str>> {
    match s {
        Schema::Object {
            properties,
            pattern_properties,
            additional: None,
        } if !properties.is_empty()
            && pattern_properties.is_empty()
            && properties
                .iter()
                .all(|p| p.required && p.schema == Schema::Any) =>
        {
            Some(properties.iter().map(|p| p.name.as_str()).collect())
        }
        _ => None,
    }
}

/// `t`, the type of object `variant` of `union`, limited to the values that
/// match none of the other variants that only require properties. A `oneOf`
/// takes a value that matches exactly one variant, and a variant that only
/// requires properties matches unless one of them is absent. So
/// `oneOf: [{required: [a]}, {required: [b, c]}]` is
/// `{ a } & ({ b?: never } | { c?: never }) | { b, c } & { a?: never }`.
fn exclusive(t: Type, variant: &Schema, union: &Schema) -> Type {
    let Schema::Union {
        kind: UnionKind::OneOf,
        variants,
        ..
    } = union
    else {
        return t;
    };
    if !matches!(variant, Schema::Object { .. }) {
        return t;
    }
    let absent = |name: &str| Type::inline(vec![Member::new(name, true, Type::name("never"))]);
    let others = variants
        .iter()
        .filter(|v| !std::ptr::eq(*v, variant))
        .filter_map(required_only)
        .map(|names| Type::Union(names.into_iter().map(absent).collect()));
    let mut parts = vec![t];
    parts.extend(others);
    if parts.len() == 1 {
        parts.pop().unwrap()
    } else {
        Type::Intersection(parts)
    }
}

fn is_bare_object(s: &Schema) -> bool {
    matches!(s, Schema::Object { properties, pattern_properties, additional: None }
        if properties.is_empty() && pattern_properties.is_empty())
}

/// An array of `item`, whose first `min` items are required: `T[]`, or
/// `[T, T, ...T[]]` for `minItems: 2`.
fn array(item: Type, min: u64) -> Type {
    if min == 0 {
        return Type::Array(Box::new(item));
    }
    Type::Tuple(
        (0..min).map(|_| item.clone()).collect(),
        Some(Box::new(item)),
    )
}

/// Whether `types` covers media-type essence `mt`, either directly or via a
/// range such as `text/*`. The runtime's `decodeBody` matches the same way.
fn covers(types: &[String], mt: &str) -> bool {
    types.iter().any(|t| {
        t == mt
            || t.strip_suffix('*')
                .is_some_and(|prefix| prefix.ends_with('/') && mt.starts_with(prefix))
    })
}

/// A `security` list as a runtime `SecurityRequirement[]` literal.
fn security_literal(security: &[SecurityRequirement]) -> Expr {
    let requirements = security
        .iter()
        .map(|r| {
            let schemes = r
                .schemes
                .iter()
                .map(|(name, scopes)| {
                    let scopes = scopes.iter().map(|s| Expr::string(s)).collect();
                    (name.clone(), Expr::Array(scopes))
                })
                .collect();
            Expr::inline(schemes)
        })
        .collect();
    Expr::Array(requirements)
}

/// Non-default serialization of a parameter, as a runtime `ParamStyle` literal.
fn param_style(p: &Param) -> Option<Expr> {
    let style = |s: &str| Expr::inline(vec![("style".into(), Expr::string(s))]);
    match &p.value {
        ParamValue::Content(c) => match p.location {
            Location::Query if c.kind() == MediaKind::Json => Some(style("json")),
            // Pre-serialized cookie values are sent as-is.
            Location::Cookie => Some(style("cookie")),
            _ => None,
        },
        ParamValue::Schema {
            style,
            explode,
            allow_reserved,
            ..
        } => style_literal(
            *style,
            *explode,
            *allow_reserved && p.location == Location::Query,
        ),
    }
}

/// A `ParamStyle` literal for a schema-based serialization, or `None` for the
/// default of its style.
fn style_literal(style: Style, explode: bool, allow_reserved: bool) -> Option<Expr> {
    let mut fields = Vec::new();
    let name = match style {
        Style::Label => Some("label"),
        Style::Matrix => Some("matrix"),
        // Exploded, the delimiter is unused and the value is sent as `form`.
        Style::SpaceDelimited if !explode => Some("spaceDelimited"),
        Style::PipeDelimited if !explode => Some("pipeDelimited"),
        Style::DeepObject => Some("deepObject"),
        Style::Cookie => Some("cookie"),
        Style::Form | Style::Simple | Style::SpaceDelimited | Style::PipeDelimited => None,
    };
    if let Some(name) = name {
        fields.push(("style".into(), Expr::string(name)));
    }
    // The delimited styles and `deepObject` have one form each at run time.
    let default_explode = match style {
        Style::Form | Style::Cookie => Some(true),
        Style::Simple | Style::Label | Style::Matrix => Some(false),
        Style::SpaceDelimited | Style::PipeDelimited | Style::DeepObject => None,
    };
    if default_explode.is_some_and(|default| explode != default) {
        fields.push(("explode".into(), Expr::Literal(explode.into())));
    }
    if allow_reserved {
        fields.push(("reserved".into(), Expr::Literal(true.into())));
    }
    (!fields.is_empty()).then(|| Expr::inline(fields))
}

/// Builds the name and type of the `statuses` alias for an operation: each
/// response key mapped to the statuses it covers as `[successes, failures]`,
/// followed by any of 204, 205 and 304 covered by a range or `default`.
/// Computing these here avoids evaluating conditional types in the runtime at
/// every call site. Since they depend only on the response keys, the alias is
/// named after those keys.
fn status_alias(op: &Operation) -> (String, Type) {
    let mut entries: Vec<(String, Vec<u16>)> = op
        .coverage()
        .into_iter()
        .filter(|(_, codes)| !codes.is_empty())
        .map(|(r, codes)| (r.map_or(Status::Default, |r| r.status).key(), codes))
        .collect();
    entries.sort();
    let mut name = String::from("Statuses");
    let mut members = Vec::new();
    for (key, codes) in entries {
        // 204, 205 and 304 never carry a body. Under their own status key the
        // response type already reflects that; under a range or `default`,
        // they are separated into a third tuple element.
        let own = key.bytes().all(|b| b.is_ascii_digit());
        let (empty, codes): (Vec<u16>, Vec<u16>) = codes
            .into_iter()
            .partition(|s| !own && matches!(s, 204 | 205 | 304));
        let (ok, failed): (Vec<u16>, Vec<u16>) =
            codes.into_iter().partition(|s| (200..300).contains(s));
        name.push('_');
        name.push_str(&key);
        let mut tuple = vec![statuses(&ok), statuses(&failed)];
        if !empty.is_empty() {
            tuple.push(statuses(&empty));
        }
        members.push(Member::new(key, false, Type::Tuple(tuple, None)));
    }
    (name, Type::object(members))
}

/// Ascending statuses as a type: `Status2xx` for a whole hundred, `Status20x`
/// for a whole ten (see [`status_runs`]), and literals for the rest.
fn statuses(codes: &[u16]) -> Type {
    let mut parts = Vec::new();
    let mut i = 0;
    while i < codes.len() {
        let s = codes[i];
        let run =
            |n: u16| s.is_multiple_of(n) && codes.get(i + usize::from(n) - 1) == Some(&(s + n - 1));
        if run(100) {
            parts.push(Type::name(format!("Status{}xx", s / 100)));
            i += 100;
        } else if run(10) {
            parts.push(Type::name(format!("Status{}x", s / 10)));
            i += 10;
        } else {
            parts.push(Type::Literal(s.into()));
            i += 1;
        }
    }
    Type::Union(parts)
}

/// Emits the 100-status and 10-status range aliases used by `statuses`, and
/// only those, because unused aliases fail `noUnusedLocals`.
fn status_runs(statuses: &BTreeMap<String, Type>) -> Vec<Item> {
    let used = |name: &str| statuses.values().any(|t| t.refers_to(name));
    let hundreds: Vec<u16> = (1..6).filter(|h| used(&format!("Status{h}xx"))).collect();
    let tens = (10..60u16).filter(|t| hundreds.contains(&(t / 10)) || used(&format!("Status{t}x")));
    let mut items: Vec<Item> = tens
        .map(|t| {
            let codes = (t * 10..t * 10 + 10).map(|c| Type::Literal(c.into()));
            Item::alias(format!("Status{t}x"), Type::Union(codes.collect()))
        })
        .collect();
    items.extend(hundreds.iter().map(|h| {
        let tens = (h * 10..h * 10 + 10).map(|t| Type::name(format!("Status{t}x")));
        Item::alias(format!("Status{h}xx"), Type::Union(tens.collect()))
    }));
    items
}

/// The members that every object inherits from `Object.prototype`.
const INHERITED: &[&str] = &[
    "constructor",
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "toLocaleString",
    "toString",
    "valueOf",
];

/// A member for a property or parameter from the document. An object that
/// leaves out an optional property named like an inherited member still has
/// that member, and TypeScript compares its type, so the type allows it.
fn property(name: &str, optional: bool, ty: Type) -> Member {
    let ty = if optional && INHERITED.contains(&name) && !ty.is("unknown") {
        let inherited = Type::name(format!("Object[{}]", quote(name)));
        if ty.is("never") {
            inherited
        } else {
            Type::Union(vec![ty, inherited])
        }
    } else {
        ty
    };
    Member::new(name, optional, ty)
}

/// The lines of the doc comment for `meta`, with `extra` after its text.
fn doc(meta: &Meta, extra: &[String]) -> Vec<String> {
    let texts = [&meta.title, &meta.summary, &meta.description];
    let mut paragraphs: Vec<Vec<String>> = texts
        .into_iter()
        .flatten()
        .map(|text| text.trim_end().lines().map(str::to_string).collect())
        .collect();
    paragraphs.push(extra.to_vec());
    paragraphs.retain(|p| !p.is_empty());
    // A blank line between paragraphs, or editors show them as one.
    let mut lines = paragraphs.join(&String::new());
    if let Some(f) = &meta.format {
        lines.push(format!("@format {f}"));
    }
    if let Some(d) = &meta.default {
        lines.push(format!("@default {d}"));
    }
    lines.extend(meta.examples.iter().map(|e| format!("@example {e}")));
    if meta.deprecated {
        lines.push("@deprecated".into());
    }
    lines
}

/// Names the emitter exports besides the runtime's and the schemas'.
const RESERVED: &[&str] = &["paths", "components", "schemas", "SecuritySchemeName"];

/// The names the runtime exports.
fn runtime_exports() -> impl Iterator<Item = String> {
    RUNTIME.lines().filter_map(|line| {
        let rest = line.strip_prefix("export ")?;
        let name = rest
            .split_whitespace()
            .find(|w| !matches!(*w, "type" | "interface" | "class" | "function" | "const"))?;
        Some(
            name.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect(),
        )
    })
}

/// The exported name of a schema: its key as an identifier.
fn export_name(key: &str) -> String {
    let name = identifier(key);
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{name}")
    } else {
        name
    }
}

/// Returns `base` (or `base_2`, `base_3`, ... if already in `taken`) and
/// records the chosen name in `taken`.
fn unique(base: String, taken: &mut HashSet<String>) -> String {
    let mut name = base.clone();
    for n in 2.. {
        if taken.insert(name.clone()) {
            break;
        }
        name = format!("{base}_{n}");
    }
    name
}

/// `key` as part of a TypeScript identifier: runs of other characters become
/// `_`, so `schemas/pet.yaml#` gives `schemas_pet_yaml`.
fn identifier(key: &str) -> String {
    let mut out = String::new();
    for c in key.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    match out.trim_matches('_') {
        "" => "_".into(),
        s => s.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use serde_json::json;

    #[test]
    fn binary_media_types_cover_their_ranges() {
        let types = ["text/csv".to_string(), "text/*".to_string()];
        assert!(covers(&types[..1], "text/csv"));
        assert!(!covers(&types[..1], "text/plain"));
        assert!(covers(&types[1..], "text/plain"));
        assert!(!covers(&types[1..], "textual/plain"));
        assert!(!covers(&types[1..], "application/json"));
    }

    fn generate(schemas: Value) -> Result<String, Error> {
        let doc = json!({"openapi": "3.2.0", "paths": {}, "components": {"schemas": schemas}});
        emit(&crate::parse(&crate::Documents::from_value(doc)?)?)
    }

    #[test]
    fn rejects_methods_the_client_cannot_send() {
        let generate = |method: &str| {
            let paths = json!({"/s": {"additionalOperations": {method: {"responses": {}}}}});
            let doc = json!({"openapi": "3.2.0", "paths": paths});
            emit(&crate::parse(&crate::Documents::from_value(doc)?)?)
        };
        for (method, message) in [
            ("with", "reserves that name"),
            ("then", "reserves that name"),
            ("post", "it is sent as `POST`; use the `post` field instead"),
            ("Get", "it is sent as `GET`; use the `get` field instead"),
        ] {
            let err = generate(method).unwrap_err().to_string();
            assert!(err.contains(message), "{method}: {err}");
        }
        for method in ["LINK", "patch", "query"] {
            assert!(generate(method).is_ok(), "{method}");
        }
    }

    #[test]
    fn rejects_parts_form_data_cannot_send_only_in_requests() {
        let generate = |op: Value| {
            let doc = json!({"openapi": "3.2.0", "paths": {"/s": {"post": op}}});
            emit(&crate::parse(&crate::Documents::from_value(doc)?)?)
        };
        let content = json!({"multipart/form-data": {"encoding": {
            "a": {"headers": {"X-Id": {"required": true, "schema": {}}}},
            "b": {"contentType": "multipart/mixed"},
        }}});
        let err = generate(json!({"requestBody": {"content": content}, "responses": {}}));
        assert!(
            err.unwrap_err()
                .to_string()
                .contains("required part header `X-Id`"),
            "request"
        );
        let ok = generate(json!({"responses": {"200": {"description": "", "content": content}}}));
        assert!(ok.is_ok(), "response");
    }

    fn r(name: &str) -> Value {
        json!({"$ref": format!("#/components/schemas/{name}")})
    }

    #[test]
    fn subtypes_get_the_parents_own_part() {
        // The shape springdoc emits: parents list their subtypes, which extend them.
        let out = generate(json!({
            "Animal": {
                "type": "object", "required": ["kind"], "properties": {"kind": {"type": "string"}},
                "discriminator": {"propertyName": "kind"}, "oneOf": [r("Dog"), r("Cat")]
            },
            "Dog": {"allOf": [r("Animal")], "oneOf": [r("Puppy"), r("Adult")]},
            "Cat": {"allOf": [r("Animal"), {"properties": {"lives": {"type": "integer"}}}]},
            "Puppy": {"allOf": [r("Dog"), {"properties": {"age": {"type": "integer"}}}]},
            "Adult": {"allOf": [r("Dog")]},
        }))
        .unwrap();
        let has = |s: &str| assert!(out.contains(s), "missing {s:?} in\n{out}");
        has(
            "type $Animal = {\n  kind: string;\n} & ($Dog & { kind: \"Dog\" } | $Cat & { kind: \"Cat\" });\n",
        );
        has("type $Dog = {\n  kind: string;\n} & ($Puppy | $Adult);\n");
        has("type $Cat = {\n  kind: string;\n} & {\n  lives?: number;\n};\n");
        // Two levels down, Puppy gets Animal's part through Dog's, without either listing.
        has("type $Puppy = {\n  kind: string;\n} & {\n  age?: number;\n};\n");
        has("type $Adult = {\n  kind: string;\n};\n");
    }

    #[test]
    fn rejects_mapping_keys_a_non_string_property_cannot_have() {
        let schemas = |key: &str| {
            json!({
                "On": {"type": "object", "properties": {"flag": {"type": "boolean"}}},
                "Off": {"type": "object", "properties": {"flag": {"type": "boolean"}}},
                "Toggle": {
                    "oneOf": [r("On"), r("Off")],
                    "discriminator": {"propertyName": "flag", "mapping": {key: "On"}}
                }
            })
        };
        assert!(generate(schemas("true")).is_ok());
        let e = generate(schemas("yes")).unwrap_err().to_string();
        assert!(e.contains("Toggle: discriminator mapping key `yes`"), "{e}");
    }

    #[test]
    fn rejects_cycles_without_an_object_or_array_in_between() {
        let err = generate(json!({"A": r("B"), "B": r("A")})).unwrap_err();
        assert_eq!(
            err.to_string(),
            "components.schemas.A: a schema cannot reference itself through $ref, allOf, oneOf or anyOf alone: A -> B -> A"
        );
        let err = generate(json!({
            "A": {"anyOf": [r("B"), {"type": "string"}]},
            "B": {"anyOf": [r("A"), {"type": "number"}]},
        }))
        .unwrap_err();
        assert!(err.to_string().ends_with(": A -> B -> A"), "{err}");
        // Through an array, the cycle is fine.
        let json = json!({"oneOf": [{"type": "string"}, {"type": "array", "items": r("Json")}]});
        assert!(
            generate(json!({"Json": json}))
                .unwrap()
                .contains("type $Json = string | $Json[];\n")
        );
    }

    #[test]
    fn rejects_cycles_the_emitter_reads_through() {
        let cycle = |extra: Value| {
            let mut schemas = json!({"A": r("B"), "B": r("A")});
            schemas
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            schemas
        };
        // A discriminating property.
        let err = generate(cycle(json!({
            "V": {"type": "object", "properties": {"k": r("A")}},
            "U": {
                "oneOf": [r("V")],
                "discriminator": {"propertyName": "k", "mapping": {"x": "V"}}
            },
        })))
        .unwrap_err();
        assert!(err.to_string().ends_with(": A -> B -> A"), "{err}");
        // A multipart body.
        let doc = json!({
            "openapi": "3.2.0",
            "paths": {"/a": {"post": {
                "requestBody": {"content": {"multipart/form-data": {"schema": r("A")}}},
                "responses": {"204": {"description": "ok"}},
            }}},
            "components": {"schemas": cycle(json!({}))},
        });
        let err = emit(&crate::parse(&crate::Documents::from_value(doc).unwrap()).unwrap());
        let err = err.unwrap_err().to_string();
        assert!(err.ends_with(": A -> B -> A"), "{err}");
    }

    #[test]
    fn runtime_exports_are_reserved() {
        let names: HashSet<String> = runtime_exports().collect();
        for n in [
            "ClientOptions",
            "ApiResponse",
            "Middleware",
            "Client",
            "DecodeError",
            "auth",
        ] {
            assert!(names.contains(n), "{n} missing from {names:?}");
        }
        let out = generate(json!({
            "Client": {"type": "string"},
            "auth": {"enum": ["a"]},
            "paths": {"type": "string"},
            "2fa": {"type": "string"},
        }))
        .unwrap();
        let has = |s: &str| assert!(out.contains(s), "missing {s:?} in\n{out}");
        has("  $Client as Client_2,\n  $paths as paths_2,\n  $2fa as _2fa,\n");
        has("const $auth = [\"a\"] as const;\nexport {\n  $auth as auth_2,\n};\n");
    }

    #[test]
    fn alias_names_are_identifiers_and_unique() {
        let out = generate(json!({
            "Pet.Owner": {"type": "string"},
            "Pet_Owner": {"type": "string"},
            "Pet-Request": {"type": "string"},
            "Pet": {"required": ["id"], "properties": {"id": {"type": "integer", "readOnly": true}}},
        }))
        .unwrap();
        let has = |s: &str| assert!(out.contains(s), "missing {s:?} in\n{out}");
        has(
            "    \"Pet.Owner\": $Pet_Owner;\n    Pet_Owner: $Pet_Owner_2;\n    \"Pet-Request\": $Pet_Request;\n    Pet: $Pet;\n",
        );
        has("  requestSchemas: {\n    Pet: $Pet$Request;\n");
        has("type $Pet$Request = {\n  id?: never;\n};\n");
    }
}
