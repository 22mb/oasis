//! TypeScript emitter: one self-contained file with types and a fetch-based runtime.

mod ast;

use crate::Error;
use crate::ir::*;
use ast::{Expr, Item, Member, Type, quote};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

const RUNTIME: &str = include_str!("typescript/runtime.ts");

pub fn emit(api: &Api) -> Result<String, Error> {
    let mut api = api.clone();
    inherit_access(&mut api);
    Emitter::new(&api).emit()
}

/// Copies `readOnly` and `writeOnly` onto every property they apply to, so that
/// the emitter can read them off the property: one whose schema is a `$ref` to
/// a flagged schema, and one that an `allOf` member names (often only in
/// `required`) while another member flags it.
fn inherit_access(api: &mut Api) {
    let owned = api.schemas.clone();
    let schemas: HashMap<&str, &NamedSchema> = owned.iter().map(|s| (s.key.as_str(), s)).collect();
    let mut fix = |s: &mut Schema| match s {
        Schema::Object { properties, .. } => {
            for p in properties {
                let (r, w) = access(&p.schema, &schemas, &mut Vec::new());
                p.meta.read_only |= r;
                p.meta.write_only |= w;
            }
        }
        Schema::Intersection(parts) => {
            let flagged = flagged_in(parts, &schemas);
            for p in parts {
                mark(p, &flagged);
            }
        }
        _ => {}
    };
    for s in &mut api.schemas {
        walk_mut(&mut s.schema, &mut fix);
    }
    for op in api.paths.iter_mut().flat_map(|p| &mut p.operations) {
        let mut contents: Vec<&mut Content> = Vec::new();
        for p in &mut op.params {
            match &mut p.value {
                ParamValue::Schema { schema, .. } => walk_mut(schema, &mut fix),
                ParamValue::Content(c) => contents.push(c),
            }
        }
        contents.extend(op.querystring.as_mut().map(|q| &mut q.content));
        contents.extend(op.body.iter_mut().flat_map(|b| &mut b.content));
        contents.extend(op.responses.iter_mut().flat_map(|r| &mut r.content));
        for c in contents {
            for s in c.schema.iter_mut().chain(&mut c.item_schema) {
                walk_mut(s, &mut fix);
            }
        }
    }
}

/// Whether a value of `s` is `readOnly` and `writeOnly` through `$ref` and
/// `allOf` alone; `visiting` guards against `$ref` cycles.
fn access<'s>(
    s: &'s Schema,
    schemas: &HashMap<&str, &'s NamedSchema>,
    visiting: &mut Vec<&'s str>,
) -> (bool, bool) {
    match s {
        Schema::Ref(k) if !visiting.contains(&k.as_str()) => {
            let Some(n) = schemas.get(k.as_str()) else {
                return (false, false);
            };
            visiting.push(k);
            let (r, w) = access(&n.schema, schemas, visiting);
            visiting.pop();
            (n.meta.read_only || r, n.meta.write_only || w)
        }
        Schema::Intersection(parts) => parts.iter().fold((false, false), |(r, w), p| {
            let (pr, pw) = access(p, schemas, visiting);
            (r || pr, w || pw)
        }),
        _ => (false, false),
    }
}

/// The names of the `readOnly` and `writeOnly` properties of `parts`, an
/// `allOf`, with nested `allOf` members and `$ref` targets flattened.
fn flagged_in(
    parts: &[Schema],
    schemas: &HashMap<&str, &NamedSchema>,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut flagged = (BTreeSet::new(), BTreeSet::new());
    for p in parts {
        flagged_properties(p, schemas, &mut flagged, &mut Vec::new());
    }
    flagged
}

/// Adds the names of the `readOnly` and `writeOnly` properties of `s`, with
/// `allOf` members and `$ref` targets flattened, to `flagged`.
fn flagged_properties<'s>(
    s: &'s Schema,
    schemas: &HashMap<&str, &'s NamedSchema>,
    flagged: &mut (BTreeSet<String>, BTreeSet<String>),
    visiting: &mut Vec<&'s str>,
) {
    match s {
        Schema::Object { properties, .. } => {
            for p in properties {
                let (r, w) = access(&p.schema, schemas, &mut Vec::new());
                if p.meta.read_only || r {
                    flagged.0.insert(p.name.clone());
                }
                if p.meta.write_only || w {
                    flagged.1.insert(p.name.clone());
                }
            }
        }
        Schema::Intersection(parts) => {
            for p in parts {
                flagged_properties(p, schemas, flagged, visiting);
            }
        }
        Schema::Ref(k) if !visiting.contains(&k.as_str()) => {
            if let Some(n) = schemas.get(k.as_str()) {
                visiting.push(k);
                flagged_properties(&n.schema, schemas, flagged, visiting);
                visiting.pop();
            }
        }
        _ => {}
    }
}

/// Flags the properties of the object members of `s` (an `allOf` member) that
/// `flagged` names.
fn mark(s: &mut Schema, flagged: &(BTreeSet<String>, BTreeSet<String>)) {
    match s {
        Schema::Object { properties, .. } => {
            for p in properties {
                p.meta.read_only |= flagged.0.contains(&p.name);
                p.meta.write_only |= flagged.1.contains(&p.name);
            }
        }
        Schema::Intersection(parts) => parts.iter_mut().for_each(|p| mark(p, flagged)),
        _ => {}
    }
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
    /// Every schema by key.
    schemas: HashMap<&'a str, &'a NamedSchema>,
    /// The type alias of each schema; its request variant adds `$Request`.
    names: HashMap<&'a str, String>,
    /// Schemas whose request view differs from their response view.
    request_variants: HashSet<&'a str>,
}

impl<'a> Emitter<'a> {
    fn new(api: &'a Api) -> Emitter<'a> {
        let schemas = api.schemas.iter().map(|s| (s.key.as_str(), s)).collect();

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
            walk(&s.schema, &mut |node| match node {
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

        let mut statuses = BTreeMap::new();
        let paths = api
            .paths
            .iter()
            .map(|item| {
                let operations = item
                    .operations
                    .iter()
                    .map(|op| self.operation(op, &mut statuses))
                    .collect();
                Member::new(&item.path, false, Type::object(operations))
            })
            .collect();
        items.push(Item::Interface {
            name: "paths".into(),
            members: paths,
        });
        items.push(Item::Blank);
        if !statuses.is_empty() {
            items.push(Item::Comment(vec![
                "The statuses each response key covers, as `[successes, failures]`; the".into(),
                "operations with the same keys share one.".into(),
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
        items.push(self.spec());
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
                ty: self.define(s, View::Response, &mut edges),
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
        self.check_discriminators()?;
        check_cycles(self.api, &edges)
    }

    /// Runtime data: default base URL, the document's methods and security, and
    /// what each operation needs beyond the defaults.
    fn spec(&self) -> Item {
        let api = self.api;
        let base_url = api
            .servers
            .first()
            .map(Server::default_url)
            .unwrap_or_default();
        let mut methods: Vec<&str> = Vec::new();
        for op in api.paths.iter().flat_map(|item| &item.operations) {
            if !methods.contains(&op.method.as_str()) {
                methods.push(&op.method);
            }
        }
        let mut operations = Vec::new();
        for item in &api.paths {
            for op in &item.operations {
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
                if let Some(ct) = op.body.as_ref().and_then(default_media_type) {
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
                let binary = self.binary_text(op);
                if !binary.is_empty() {
                    let types = binary.iter().map(|mt| Expr::string(mt)).collect();
                    info.push(("binary".into(), Expr::Array(types)));
                }
                if !info.is_empty() {
                    operations.push((format!("{} {}", op.method, item.path), Expr::inline(info)));
                }
            }
        }
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

    /// One `paths` member; its `statuses` alias goes to `statuses` (name -> type).
    fn operation(&self, op: &Operation, statuses: &mut BTreeMap<String, Type>) -> Member {
        let mut extra = Vec::new();
        if let Some(id) = &op.operation_id {
            extra.push(format!("operationId: {id}"));
        }
        let mut responses = Vec::new();
        if op.responses.is_empty() {
            responses.push(Member::new("default", false, Type::name("unknown")));
        }
        let binary = self.binary_text(op);
        for r in &op.responses {
            // Never carries a body on the wire, whatever the document declares.
            let bodiless = op.method == "HEAD" || matches!(r.status, Status::Code(204 | 205 | 304));
            let data = if bodiless || r.content.is_empty() {
                Type::name("undefined")
            } else {
                Type::Union(
                    r.content
                        .iter()
                        .map(|c| self.response_data(c, &binary))
                        .collect(),
                )
            };
            responses.push(Member::new(r.status.key(), false, data).doc(doc(&r.meta, &[])));
        }
        let (name, ty) = status_alias(op);
        let members = vec![
            Member::new("request", false, self.request(op)),
            Member::new("responses", false, Type::object(responses)),
            Member::new("statuses", false, Type::name(&name)),
        ];
        statuses.entry(name).or_insert(ty);
        Member::new(&op.method, false, Type::object(members)).doc(doc(&op.meta, &extra))
    }

    fn request(&self, op: &Operation) -> Type {
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
                    Member::new(&p.name, !p.required, t).doc(doc(&p.meta, &[]))
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
            let default = default_media_type(body);
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
                let body_field = Member::new("body", !body.required, self.request_body(c));
                variants.push(vec![ct, body_field]);
            }
            // An optional body can be left out without naming a media type, also
            // when no media type is implied.
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
            MediaKind::Multipart => match c.schema.as_ref().map(|s| (s, self.resolve(s))) {
                Some((
                    _,
                    Schema::Object {
                        properties,
                        pattern_properties,
                        additional,
                    },
                )) if essence(&c.media_type) == "multipart/form-data" => self.object(
                    properties,
                    pattern_properties,
                    additional.as_deref(),
                    View::Request,
                    &|s| self.part(s),
                ),
                Some((s, other))
                    if *other != Schema::Any && essence(&c.media_type) == "multipart/form-data" =>
                {
                    self.ty(s, View::Request)
                }
                _ if essence(&c.media_type) == "multipart/form-data" => Type::name("FormData"),
                // `FormData` is always sent as `multipart/form-data`. A `Blob` whose
                // `type` names the media type and its boundary becomes `Content-Type`.
                _ => Type::name("Blob"),
            },
            MediaKind::Text => Type::name("string"),
            // Sent as it is. Fetch encodes these two in formats of their own.
            _ => Type::Generic(
                "Exclude",
                vec![
                    Type::name("BodyInit"),
                    Type::Union(vec![Type::name("FormData"), Type::name("URLSearchParams")]),
                ],
            ),
        }
    }

    /// A property of a `multipart/form-data` body: a `Blob` where its schema has
    /// no `type`, which is how binary data is described, and an array of them
    /// for an array, whose items are sent as parts of the same name.
    fn part(&self, s: &Schema) -> Type {
        match self.resolve(s) {
            Schema::Any => Type::name("Blob"),
            Schema::Array {
                prefix_items,
                items,
                min_items,
            } if prefix_items.is_empty()
                && items
                    .as_deref()
                    .is_none_or(|i| *self.resolve(i) == Schema::Any) =>
            {
                array(Type::name("Blob"), *min_items)
            }
            _ => self.ty(s, View::Request),
        }
    }

    /// `s`, with references followed. A cycle of references, which
    /// [`check_cycles`] rejects later on, ends at one of its references.
    fn resolve<'s>(&'s self, mut s: &'s Schema) -> &'s Schema {
        for _ in 0..=self.schemas.len() {
            let Schema::Ref(k) = s else { break };
            s = &self.schemas[k.as_str()].schema;
        }
        s
    }

    /// The runtime `Encoding` of each property of a form body that needs one:
    /// those the document gives an Encoding Object, and the `multipart` parts
    /// of encoded binary strings, which are not text.
    fn encoding_literal(&self, c: &Content) -> Option<Expr> {
        let multipart = essence(&c.media_type) == "multipart/form-data";
        let mut fields: Vec<(String, Expr)> = c
            .encoding
            .iter()
            .map(|(name, e)| {
                let e = match e {
                    Encoding::ContentType(ct) => ("contentType".into(), Expr::string(ct)),
                    Encoding::Style {
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
                (name.clone(), Expr::inline(vec![e]))
            })
            .collect();
        if multipart
            && let Some(Schema::Object { properties, .. }) =
                c.schema.as_ref().map(|s| self.resolve(s))
        {
            for p in properties {
                let encoded = |s: &Schema| {
                    matches!(
                        self.resolve(s),
                        Schema::String {
                            content_encoding: Some(_),
                            ..
                        }
                    )
                };
                let binary = match self.resolve(&p.schema) {
                    Schema::Array { items: Some(i), .. } => encoded(i),
                    s => encoded(s),
                };
                if binary && !c.encoding.iter().any(|(name, _)| *name == p.name) {
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

    /// The `text/*` response media types of `op` whose schema says the body is
    /// binary (no `type`, or `format: binary` in 3.0), as essences. The runtime
    /// reads a response of one of them as a `Blob` instead of text, whichever
    /// response declares it, since it decodes by the actual `Content-Type`.
    fn binary_text(&self, op: &Operation) -> Vec<String> {
        let mut found = Vec::new();
        for c in op.responses.iter().flat_map(|r| &r.content) {
            let binary = c.kind() == MediaKind::Text
                && c.schema
                    .as_ref()
                    .is_some_and(|s| *self.resolve(s) == Schema::Any);
            let mt = essence(&c.media_type);
            if binary && !found.contains(&mt) {
                found.push(mt);
            }
        }
        found
    }

    /// `binary` is [`Emitter::binary_text`] of the operation.
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
                    .map(|v| self.tag(self.ty(v, view), v, discriminator.as_ref()))
                    .collect(),
            ),
            Schema::Intersection(parts) => {
                let hidden = self.hidden_in(parts, view);
                Type::Intersection(
                    parts
                        .iter()
                        .map(|p| {
                            let t = self.composed(p, view);
                            match p {
                                Schema::Ref(_) => self.omit(t, p, &hidden, view),
                                _ => t,
                            }
                        })
                        .collect(),
                )
            }
        }
    }

    /// The properties of `parts`, an `allOf`, that are absent from `view`:
    /// `readOnly` ones from requests, `writeOnly` ones from responses.
    fn hidden_in(&self, parts: &[Schema], view: View) -> BTreeSet<String> {
        let (read_only, write_only) = flagged_in(parts, &self.schemas);
        match view {
            View::Request => read_only,
            View::Response => write_only,
        }
    }

    /// `t`, the type of `$ref` member `s` of an `allOf`, without the properties
    /// in `hidden` that it shows in `view`. Another member flags them
    /// `readOnly` or `writeOnly`, so they are `?: never` in this view, but the
    /// shared target cannot be flagged and may require them: `number & never`
    /// would make every value fail. A union target keeps its type, since
    /// `Omit` would merge its members, and so does one with an index
    /// signature, which `Omit` would keep alone.
    fn omit(&self, t: Type, s: &Schema, hidden: &BTreeSet<String>, view: View) -> Type {
        let mut declared = HashSet::new();
        if hidden.is_empty() || !self.declared(s, view, &mut declared, &mut Vec::new()) {
            return t;
        }
        let names: Vec<Type> = hidden
            .iter()
            .filter(|n| declared.contains(n.as_str()))
            .map(|n| Type::string(n))
            .collect();
        match names.len() {
            0 => t,
            1 => Type::Generic("Omit", vec![t, names.into_iter().next().unwrap()]),
            _ => Type::Generic("Omit", vec![t, Type::Union(names)]),
        }
    }

    /// Adds the names of the properties `s` declares and shows in `view`,
    /// through `$ref` and `allOf`, to `names`; false where `s` is or contains a
    /// union or an object with an index signature.
    fn declared<'s>(
        &'s self,
        s: &'s Schema,
        view: View,
        names: &mut HashSet<&'s str>,
        visiting: &mut Vec<&'s str>,
    ) -> bool {
        match s {
            Schema::Ref(k) if visiting.contains(&k.as_str()) => true,
            Schema::Ref(k) => {
                visiting.push(k);
                let target = &self.schemas[k.as_str()].schema;
                let ok = self.declared(target, view, names, visiting);
                visiting.pop();
                ok
            }
            Schema::Object {
                pattern_properties,
                additional,
                ..
            } if !pattern_properties.is_empty()
                || additional.as_deref().is_some_and(|a| *a != Schema::Never) =>
            {
                false
            }
            Schema::Object { properties, .. } => {
                let shown = properties.iter().filter(|p| match view {
                    View::Request => !p.meta.read_only,
                    View::Response => !p.meta.write_only,
                });
                names.extend(shown.map(|p| p.name.as_str()));
                true
            }
            Schema::Intersection(parts) => parts
                .iter()
                .all(|p| self.declared(p, view, names, visiting)),
            Schema::Union { .. } => false,
            _ => true,
        }
    }

    /// The type of a member of an intersection. There, `type: object` without
    /// other keywords only says the value is an object: as an index signature
    /// it would accept every property name the other members leave out.
    fn composed(&self, s: &Schema, view: View) -> Type {
        match s {
            Schema::Object {
                properties,
                pattern_properties,
                additional: None,
            } if properties.is_empty() && pattern_properties.is_empty() => Type::name("object"),
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
            // covers every key, with the types of the patterns and of
            // `additionalProperties` that are not closed.
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
                Member::new(&p.name, !p.required || absent, t).doc(doc(&p.meta, &[]))
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
            .property(v, &d.property_name)
            .filter(|p| !self.is_text(p));
        // The default variant takes every value the mapping does not list, and
        // values without the property; its own schema says whether it is required.
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
                    Some(p) => self.mapped_value(p, value).map(Type::Literal),
                    None => Some(Type::string(value)),
                })
                .collect();
            if values.is_empty() {
                return t;
            }
            (false, Type::Union(values))
        };
        let tag = Member::new(&d.property_name, optional, values);
        Type::Intersection(vec![t, Type::inline(vec![tag])])
    }

    /// The schema of property `name` of `s`, through references and `allOf`.
    fn property(&self, s: &'a Schema, name: &str) -> Option<&'a Schema> {
        let mut stack = vec![s];
        let mut seen = HashSet::new();
        while let Some(s) = stack.pop() {
            match s {
                Schema::Ref(k) => {
                    if seen.insert(k.as_str()) {
                        stack.push(&self.schemas[k.as_str()].schema);
                    }
                }
                Schema::Object { properties, .. } => {
                    if let Some(p) = properties.iter().find(|p| p.name == name) {
                        return Some(&p.schema);
                    }
                }
                Schema::Intersection(parts) => stack.extend(parts.iter().rev()),
                _ => {}
            }
        }
        None
    }

    /// Whether property schema `p` allows strings, so that a mapping key can
    /// stand for itself.
    fn is_text(&self, p: &Schema) -> bool {
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

    /// The value of a property without strings, schema `p`, that mapping key
    /// `key` stands for.
    fn mapped_value(&self, p: &Schema, key: &str) -> Option<Value> {
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

    /// Rejects a mapping key that no value of its non-string property can have,
    /// which could never match. A key that is the name of its schema may be the
    /// implicit one, which only applies to strings.
    fn check_discriminators(&self) -> Result<(), Error> {
        let mut result = Ok(());
        for s in &self.api.schemas {
            walk(&s.schema, &mut |node| {
                let Schema::Union {
                    variants,
                    discriminator: Some(d),
                    ..
                } = node
                else {
                    return;
                };
                for v in variants {
                    let Schema::Ref(k) = v else { continue };
                    let Some(p) = self
                        .property(v, &d.property_name)
                        .filter(|p| !self.is_text(p))
                    else {
                        continue;
                    };
                    let bad = d.mapping.iter().find(|(value, target)| {
                        target == k && value != target && self.mapped_value(p, value).is_none()
                    });
                    if let (Some((value, _)), Ok(())) = (bad, &result) {
                        result = Err(Error::new(format!(
                            "{}: discriminator mapping key `{value}` is not a value of property `{}`",
                            s.key, d.property_name
                        ))
                        .about_schema(&s.key));
                    }
                }
            });
        }
        result
    }

    /// The type of a named schema, for its alias.
    ///
    /// TypeScript resolves the top of an alias eagerly: a cycle through `$ref`,
    /// `allOf`, `oneOf` and `anyOf` alone is an error (TS2456), while one
    /// through an object or array is fine. A parent that lists its subtypes in
    /// `oneOf` while each subtype extends it with `allOf` (as springdoc emits)
    /// is such a cycle. It is cut on the subtype's side: there, the reference
    /// to the parent becomes the parent's own part, its type without the
    /// listing. References left in eager positions go to `edges`, for
    /// [`check_cycles`].
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

    /// The intersection members of `s` at the top of the alias of `d.key`.
    /// `member` says `s` is an `allOf` member (or the whole alias), where a
    /// reference to a parent listing `d.key` is replaced by the parent's own
    /// part. While one is being rendered, the listing itself is dropped.
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
                        self.tag(t, v, discriminator.as_ref())
                    })
                    .collect();
                vec![Type::Union(types)]
            }
            Schema::Intersection(parts) => {
                let hidden = self.hidden_in(parts, view);
                let mut types = Vec::new();
                for p in parts {
                    match p {
                        Schema::Object { .. }
                        | Schema::Union {
                            kind: UnionKind::Types,
                            ..
                        } => types.push(self.composed(p, view)),
                        Schema::Ref(k) if !self.enumerates(k, d.key) => types.extend(
                            self.eager(p, d, true)
                                .into_iter()
                                .map(|t| self.omit(t, p, &hidden, view)),
                        ),
                        _ => types.extend(self.eager(p, d, true)),
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
    fn reaches(&self, s: &'a Schema, c: &str) -> bool {
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
                    return Err(Error::new(format!(
                        "{}: a schema cannot reference itself through $ref, allOf, oneOf or anyOf alone",
                        cycle.join(" -> ")
                    ))
                    .about_schema(to));
                }
                Some(_) => {}
            }
        }
    }
    Ok(())
}

/// Visits a schema tree without following references.
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

fn walk<'s>(s: &'s Schema, f: &mut dyn FnMut(&'s Schema)) {
    f(s);
    match s {
        Schema::Array {
            prefix_items,
            items,
            ..
        } => {
            prefix_items.iter().for_each(|c| walk(c, f));
            if let Some(c) = items {
                walk(c, f);
            }
        }
        Schema::Object {
            properties,
            pattern_properties,
            additional,
        } => {
            properties.iter().for_each(|p| walk(&p.schema, f));
            pattern_properties.iter().for_each(|(_, c)| walk(c, f));
            if let Some(c) = additional {
                walk(c, f);
            }
        }
        Schema::Union { variants, .. } | Schema::Intersection(variants) => {
            variants.iter().for_each(|c| walk(c, f))
        }
        _ => {}
    }
}

/// The media type a request body is sent as when the call names none: the
/// only one declared, or else the first `application/json` one.
/// Whether media type `mt`, an essence, is in `types`, itself or within a
/// range such as `text/*`. The runtime's `decodeBody` matches the same way.
fn covers(types: &[String], mt: &str) -> bool {
    types.iter().any(|t| {
        t == mt
            || t.strip_suffix('*')
                .is_some_and(|prefix| prefix.ends_with('/') && mt.starts_with(prefix))
    })
}

fn default_media_type(body: &Body) -> Option<&str> {
    let declared: Vec<&str> = body
        .content
        .iter()
        .map(|c| c.media_type.as_str())
        .filter(|m| !essence(m).contains('*'))
        .collect();
    match declared.as_slice() {
        [only] if body.content.len() == 1 => Some(only),
        _ => declared
            .into_iter()
            .find(|m| essence(m) == "application/json"),
    }
}

/// [`walk`], with each node mutable before its children are visited.
fn walk_mut(s: &mut Schema, f: &mut dyn FnMut(&mut Schema)) {
    f(s);
    match s {
        Schema::Array {
            prefix_items,
            items,
            ..
        } => {
            prefix_items.iter_mut().for_each(|c| walk_mut(c, f));
            if let Some(c) = items {
                walk_mut(c, f);
            }
        }
        Schema::Object {
            properties,
            pattern_properties,
            additional,
        } => {
            properties
                .iter_mut()
                .for_each(|p| walk_mut(&mut p.schema, f));
            pattern_properties
                .iter_mut()
                .for_each(|(_, c)| walk_mut(c, f));
            if let Some(c) = additional {
                walk_mut(c, f);
            }
        }
        Schema::Union { variants, .. } | Schema::Intersection(variants) => {
            variants.iter_mut().for_each(|c| walk_mut(c, f))
        }
        _ => {}
    }
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
    match style {
        Style::DeepObject => fields.push(("style".into(), Expr::string("deepObject"))),
        Style::Cookie => fields.push(("style".into(), Expr::string("cookie"))),
        Style::Form | Style::Simple => {}
    }
    let default_explode = style != Style::Simple;
    if explode != default_explode && style != Style::DeepObject {
        fields.push(("explode".into(), Expr::Literal(explode.into())));
    }
    if allow_reserved {
        fields.push(("reserved".into(), Expr::Literal(true.into())));
    }
    (!fields.is_empty()).then(|| Expr::inline(fields))
}

/// The name and type of the alias for an operation's `statuses`: each response
/// key with the statuses it covers, as `[successes, failures]`. These are
/// computed here rather than by conditional types in the runtime, which would
/// cost every call site; they depend only on the keys, which name the alias.
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
        let (ok, failed): (Vec<u16>, Vec<u16>) =
            codes.into_iter().partition(|s| (200..300).contains(s));
        name.push('_');
        name.push_str(&key);
        let pair = Type::Tuple(vec![statuses(&ok), statuses(&failed)], None);
        members.push(Member::new(key, false, pair));
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

/// The aliases for a whole hundred and a whole ten of statuses that the
/// `statuses` aliases use, and only those: an unused one fails `noUnusedLocals`.
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

/// The lines of the doc comment for `meta`, with `extra` after its text.
fn doc(meta: &Meta, extra: &[String]) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for text in [&meta.title, &meta.summary, &meta.description]
        .into_iter()
        .flatten()
    {
        lines.extend(text.trim_end().lines().map(str::to_string));
    }
    lines.extend(extra.iter().cloned());
    if let Some(d) = &meta.default {
        lines.push(format!("@default {d}"));
    }
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

/// `base`, or `base_2`, `base_3`, ... when it is taken; the result is taken.
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
            "A -> B -> A: a schema cannot reference itself through $ref, allOf, oneOf or anyOf alone"
        );
        let err = generate(json!({
            "A": {"anyOf": [r("B"), {"type": "string"}]},
            "B": {"anyOf": [r("A"), {"type": "number"}]},
        }))
        .unwrap_err();
        assert!(err.to_string().starts_with("A -> B -> A:"), "{err}");
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
        assert!(err.to_string().starts_with("A -> B -> A:"), "{err}");
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
        assert!(err.starts_with("A -> B -> A:"), "{err}");
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
            "Pet$Request": {"type": "string"},
            "Pet": {"required": ["id"], "properties": {"id": {"type": "integer", "readOnly": true}}},
        }))
        .unwrap();
        let has = |s: &str| assert!(out.contains(s), "missing {s:?} in\n{out}");
        has(
            "    \"Pet.Owner\": $Pet_Owner;\n    Pet_Owner: $Pet_Owner_2;\n    Pet$Request: $Pet_Request;\n    Pet: $Pet;\n",
        );
        has("  requestSchemas: {\n    Pet: $Pet$Request;\n");
        has("type $Pet$Request = {\n  id?: never;\n};\n");
    }
}
