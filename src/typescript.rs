//! TypeScript emitter: one self-contained file with types and a fetch-based runtime.

use crate::Error;
use crate::ir::*;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

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
    let schemas: HashMap<String, NamedSchema> = api
        .schemas
        .iter()
        .map(|s| (s.key.clone(), s.clone()))
        .collect();
    let mut fix = |s: &mut Schema| match s {
        Schema::Object { properties, .. } => {
            for p in properties {
                let (r, w) = access(&p.schema, &schemas, &mut Vec::new());
                p.meta.read_only |= r;
                p.meta.write_only |= w;
            }
        }
        Schema::Intersection(parts) => {
            let mut flagged = (HashSet::new(), HashSet::new());
            for p in parts.iter() {
                flagged_properties(p, &schemas, &mut flagged, &mut Vec::new());
            }
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
    schemas: &'s HashMap<String, NamedSchema>,
    visiting: &mut Vec<&'s str>,
) -> (bool, bool) {
    match s {
        Schema::Ref(k) if !visiting.contains(&k.as_str()) => {
            let Some(n) = schemas.get(k) else {
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

/// Adds the names of the `readOnly` and `writeOnly` properties of `s`, with
/// `allOf` members and `$ref` targets flattened, to `flagged`.
fn flagged_properties<'s>(
    s: &'s Schema,
    schemas: &'s HashMap<String, NamedSchema>,
    flagged: &mut (HashSet<String>, HashSet<String>),
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
            if let Some(n) = schemas.get(k) {
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
fn mark(s: &mut Schema, flagged: &(HashSet<String>, HashSet<String>)) {
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
            let base = format!("${}", identifier(&s.key));
            let mut name = base.clone();
            for n in 2.. {
                if taken.insert(name.clone()) {
                    break;
                }
                name = format!("{base}_{n}");
            }
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
        let mut out = String::new();
        let source = [api.title.as_str(), api.version.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        writeln!(
            out,
            "// Generated by oasis-gen from {}. Do not edit.",
            quote(&source)
        )
        .unwrap();
        out.push('\n');

        out.push_str("export interface paths {\n");
        for item in &api.paths {
            writeln!(out, "  {}: {{", quote(&item.path)).unwrap();
            for op in &item.operations {
                self.operation(&mut out, op);
            }
            out.push_str("  };\n");
        }
        out.push_str("}\n\n");

        out.push_str("export interface components {\n");
        self.schema_group(&mut out, "schemas", false, View::Response);
        self.schema_group(&mut out, "requestSchemas", false, View::Request);
        self.schema_group(&mut out, "refs", true, View::Response);
        self.schema_group(&mut out, "requestRefs", true, View::Request);
        out.push_str("}\n\n");
        out.push_str("export type schemas = components[\"schemas\"];\n\n");
        self.definitions(&mut out)?;
        let names: Vec<String> = api.security_schemes.iter().map(|s| quote(&s.key)).collect();
        writeln!(
            out,
            "/** Security schemes declared in the document. */\nexport type SecuritySchemeName = {};\n",
            union(names)
        )
        .unwrap();

        out.push_str(RUNTIME);
        out.push('\n');
        self.spec(&mut out);
        out.push_str(
            "export function createClient(options: ClientOptions = {}): Client<paths> {\n  \
               return makeClient<paths>(options, spec);\n\
             }\n",
        );
        Ok(out)
    }

    /// One `components` member: component or hoisted schemas, in one view.
    fn schema_group(&self, out: &mut String, name: &str, hoisted: bool, view: View) {
        let schemas: Vec<&NamedSchema> = self
            .api
            .schemas
            .iter()
            .filter(|s| s.hoisted == hoisted)
            .filter(|s| view == View::Response || self.request_variants.contains(s.key.as_str()))
            .collect();
        if schemas.is_empty() {
            writeln!(out, "  {name}: {{}};").unwrap();
            return;
        }
        writeln!(out, "  {name}: {{").unwrap();
        for s in schemas {
            writeln!(
                out,
                "    {}: {};",
                key(&s.key),
                self.reference(&s.key, view)
            )
            .unwrap();
        }
        out.push_str("  };\n");
    }

    /// One type alias per schema, and one for its request view where that differs.
    fn definitions(&self, out: &mut String) -> Result<(), Error> {
        if self.api.schemas.is_empty() {
            return Ok(());
        }
        out.push_str(
            "// One type alias per schema, `$` plus its name; `$<name>$Request` is the request\n\
             // shape where readOnly or writeOnly properties make it differ.\n",
        );
        let mut edges = Vec::new();
        for s in &self.api.schemas {
            let key = s.key.as_str();
            let doc = jsdoc(&s.meta, &[], 0);
            out.push_str(&doc);
            let name = self.reference(key, View::Response);
            writeln!(
                out,
                "type {name} = {};",
                self.define(s, View::Response, &mut edges)
            )
            .unwrap();
            if self.request_variants.contains(key) {
                out.push_str(&doc);
                let name = self.reference(key, View::Request);
                writeln!(
                    out,
                    "type {name} = {};",
                    self.define(s, View::Request, &mut Vec::new())
                )
                .unwrap();
            }
        }
        out.push('\n');
        check_cycles(self.api, &edges)
    }

    /// Runtime data: default base URL, the document's methods, per-operation
    /// servers and non-default parameter styles.
    fn spec(&self, out: &mut String) {
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
        let methods = methods.iter().map(|m| quote(m)).collect::<Vec<_>>();
        let mut servers = String::new();
        let mut styles = String::new();
        let mut operations = String::new();
        for item in &api.paths {
            for op in &item.operations {
                let op_key = quote(&format!("{} {}", op.method, item.path));
                if let Some(s) = op.servers.first().or(item.servers.first()) {
                    writeln!(servers, "    {op_key}: {},", quote(&s.default_url())).unwrap();
                }
                let entries: Vec<String> = op
                    .params
                    .iter()
                    .filter_map(|p| {
                        let style = param_style(p)?;
                        let name = format!("{}:{}", p.location.as_str(), p.name);
                        Some(format!("{}: {style}", quote(&name)))
                    })
                    .collect();
                if !entries.is_empty() {
                    writeln!(styles, "    {op_key}: {{ {} }},", entries.join(", ")).unwrap();
                }
                let mut info = Vec::new();
                if let Some(id) = &op.operation_id {
                    info.push(format!("operationId: {}", quote(id)));
                }
                if let Some(security) = &op.security {
                    info.push(format!("security: {}", security_literal(security)));
                }
                if let Some(ct) = op.body.as_ref().and_then(default_media_type) {
                    info.push(format!("contentType: {}", quote(ct)));
                }
                if !info.is_empty() {
                    writeln!(operations, "    {op_key}: {{ {} }},", info.join(", ")).unwrap();
                }
            }
        }
        let block = |s: String| {
            if s.is_empty() {
                "{}".to_string()
            } else {
                format!("{{\n{s}  }}")
            }
        };
        let mut schemes = String::new();
        for s in &api.security_schemes {
            let spec = match &s.kind {
                SecuritySchemeKind::ApiKey { name, location } => {
                    let location = match location {
                        ApiKeyLocation::Query => "query",
                        ApiKeyLocation::Header => "header",
                        ApiKeyLocation::Cookie => "cookie",
                    };
                    format!(
                        "{{ type: \"apiKey\", in: \"{location}\", name: {} }}",
                        quote(name)
                    )
                }
                SecuritySchemeKind::Http { scheme, .. } => {
                    format!("{{ type: \"http\", scheme: {} }}", quote(scheme))
                }
                SecuritySchemeKind::MutualTls => "{ type: \"mutualTLS\" }".into(),
                SecuritySchemeKind::OAuth2 { .. } => "{ type: \"oauth2\" }".into(),
                SecuritySchemeKind::OpenIdConnect { .. } => "{ type: \"openIdConnect\" }".into(),
            };
            writeln!(schemes, "    {}: {spec},", key(&s.key)).unwrap();
        }
        writeln!(
            out,
            "const spec: Spec = {{\n  baseUrl: {},\n  methods: [{}],\n  servers: {},\n  styles: {},\n  security: {},\n  securitySchemes: {},\n  operations: {},\n}};\n",
            quote(&base_url),
            methods.join(", "),
            block(servers),
            block(styles),
            security_literal(&api.security),
            block(schemes),
            block(operations)
        )
        .unwrap();
    }

    fn operation(&self, out: &mut String, op: &Operation) {
        let mut extra = Vec::new();
        if let Some(id) = &op.operation_id {
            extra.push(format!("operationId: {id}"));
        }
        out.push_str(&jsdoc(&op.meta, &extra, 2));
        writeln!(out, "    {}: {{", key(&op.method)).unwrap();
        writeln!(out, "      request: {};", self.request(op)).unwrap();
        out.push_str("      responses: {\n");
        if op.responses.is_empty() {
            out.push_str("        default: unknown;\n");
        }
        for r in &op.responses {
            // Never carries a body on the wire, whatever the document declares.
            let bodiless = op.method == "HEAD" || matches!(r.status, Status::Code(204 | 205 | 304));
            let data = if bodiless || r.content.is_empty() {
                "undefined".to_string()
            } else {
                union(r.content.iter().map(|c| self.response_data(c)).collect())
            };
            out.push_str(&jsdoc(&r.meta, &[], 4));
            writeln!(out, "        {}: {};", quote(&r.status.key()), data).unwrap();
        }
        out.push_str("      };\n    };\n");
    }

    fn request(&self, op: &Operation) -> String {
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
            let mut obj = String::from("{\n");
            for p in &params {
                obj.push_str(&jsdoc(&p.meta, &[], 5));
                let t = match &p.value {
                    ParamValue::Schema { schema, .. } => self.ty(schema, 5, View::Request),
                    ParamValue::Content(c)
                        if loc == Location::Query && c.kind() == MediaKind::Json =>
                    {
                        self.schema_or_unknown(c.schema.as_ref(), 5, View::Request)
                    }
                    // Other content-based parameters are passed pre-serialized.
                    ParamValue::Content(_) => "string".to_string(),
                };
                writeln!(
                    obj,
                    "{}{}{}: {};",
                    pad(5),
                    key(&p.name),
                    opt(!p.required),
                    t
                )
                .unwrap();
            }
            obj.push_str(&pad(4));
            obj.push('}');
            let required = params.iter().any(|p| p.required);
            fields.push(format!("{}{}: {}", loc.as_str(), opt(!required), obj));
        }
        if let Some(qs) = &op.querystring {
            let t = match (qs.content.kind(), &qs.content.schema) {
                (MediaKind::FormUrlEncoded, Some(s)) if *s != Schema::Any => {
                    self.ty(s, 4, View::Request)
                }
                (MediaKind::FormUrlEncoded, _) => "Record<string, unknown>".to_string(),
                _ => "string".to_string(),
            };
            fields.push(format!("querystring{}: {}", opt(!qs.required), t));
        }

        // One set of fields per request media type; several become a union.
        let mut variants: Vec<Vec<String>> = Vec::new();
        if let Some(body) = &op.body {
            for c in &body.content {
                let ct = if c.media_type.contains('*') {
                    "contentType?: string".to_string()
                } else {
                    let implied = default_media_type(body) == Some(c.media_type.as_str());
                    format!("contentType{}: {}", opt(implied), quote(&c.media_type))
                };
                let body_field = format!("body{}: {}", opt(!body.required), self.request_body(c));
                variants.push(vec![ct, body_field]);
            }
        }

        let object = |fields: &[String]| {
            if fields.is_empty() {
                return "{}".to_string();
            }
            let mut s = String::from("{\n");
            for f in fields {
                writeln!(s, "{}{f};", pad(4)).unwrap();
            }
            s.push_str(&pad(3));
            s.push('}');
            s
        };
        match variants.len() {
            0 => object(&fields),
            1 => {
                fields.append(&mut variants[0]);
                object(&fields)
            }
            _ => {
                let bodies = variants
                    .iter()
                    .map(|v| object(v))
                    .collect::<Vec<_>>()
                    .join(" | ");
                if fields.is_empty() {
                    bodies
                } else {
                    format!("{} & ({bodies})", object(&fields))
                }
            }
        }
    }

    fn request_body(&self, c: &Content) -> String {
        match c.kind() {
            MediaKind::Json | MediaKind::FormUrlEncoded | MediaKind::Unknown => {
                self.schema_or_unknown(c.schema.as_ref(), 4, View::Request)
            }
            MediaKind::Multipart => "FormData".to_string(),
            MediaKind::Text => "string".to_string(),
            _ => "BodyInit".to_string(),
        }
    }

    fn response_data(&self, c: &Content) -> String {
        let kind = c.kind();
        if kind.is_sequential() {
            let item = match (kind, c.sequence_item()) {
                (_, Some(s)) => self.ty(s, 4, View::Response),
                (MediaKind::EventStream, None) => "ServerSentEvent".to_string(),
                _ => "unknown".to_string(),
            };
            return format!("AsyncIterable<{item}>");
        }
        match kind {
            MediaKind::Json => self.schema_or_unknown(c.schema.as_ref(), 4, View::Response),
            MediaKind::Text | MediaKind::FormUrlEncoded => "string".to_string(),
            // A range such as `*/*` is decoded by the actual Content-Type.
            MediaKind::Unknown => "unknown".to_string(),
            _ => "Blob".to_string(),
        }
    }

    fn schema_or_unknown(&self, s: Option<&Schema>, indent: usize, view: View) -> String {
        s.map(|s| self.ty(s, indent, view))
            .unwrap_or_else(|| "unknown".to_string())
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

    /// TypeScript type for a schema. `indent` is the nesting level of the line it starts on.
    fn ty(&self, s: &Schema, indent: usize, view: View) -> String {
        match s {
            Schema::Any => "unknown".into(),
            Schema::Never => "never".into(),
            Schema::Ref(k) => self.reference(k, view),
            Schema::Null => "null".into(),
            Schema::Boolean => "boolean".into(),
            Schema::Integer { .. } | Schema::Number { .. } => "number".into(),
            Schema::String { .. } => "string".into(),
            Schema::Const { value, .. } => literal(value),
            Schema::Enum { values, .. } => union(values.iter().map(literal).collect()),
            Schema::Array {
                prefix_items,
                items,
            } => {
                if prefix_items.is_empty() {
                    let item = match items.as_deref() {
                        Some(s) => wrap(s, self.ty(s, indent, view)),
                        None => "unknown".into(),
                    };
                    return format!("{item}[]");
                }
                let mut elems: Vec<String> = prefix_items
                    .iter()
                    .map(|s| self.ty(s, indent, view))
                    .collect();
                match items.as_deref() {
                    Some(Schema::Never) => {}
                    Some(rest) => {
                        elems.push(format!("...{}[]", wrap(rest, self.ty(rest, indent, view))))
                    }
                    None => elems.push("...unknown[]".into()),
                }
                format!("[{}]", elems.join(", "))
            }
            Schema::Object {
                properties,
                additional,
            } => {
                let index = match (properties.is_empty(), additional.as_deref()) {
                    (_, Some(Schema::Never)) => None,
                    (true, Some(s)) => Some(self.ty(s, indent + 1, view)),
                    (true, None) => Some("unknown".to_string()),
                    // Index signatures must cover every declared property type.
                    (false, Some(_)) => Some("unknown".to_string()),
                    (false, None) => None,
                };
                if properties.is_empty() {
                    return match index {
                        Some(t) => format!("{{ [key: string]: {t} }}"),
                        None => "Record<string, never>".into(),
                    };
                }
                let mut out = String::from("{\n");
                for p in properties {
                    // A readOnly property should not be sent in a request, nor a
                    // writeOnly one in a response.
                    let absent = (view == View::Request && p.meta.read_only)
                        || (view == View::Response && p.meta.write_only);
                    let optional = !p.required || absent;
                    // A required property can be anything but undefined; this also makes
                    // `{ id: {} | null } & Base` narrow `Base["id"]` instead of widening it.
                    let t = match &p.schema {
                        _ if absent => "never".to_string(),
                        Schema::Any if p.required => "{} | null".to_string(),
                        s => self.ty(s, indent + 1, view),
                    };
                    out.push_str(&jsdoc(&p.meta, &[], indent + 1));
                    writeln!(
                        out,
                        "{}{}{}: {t};",
                        pad(indent + 1),
                        key(&p.name),
                        opt(optional)
                    )
                    .unwrap();
                }
                if let Some(t) = index {
                    writeln!(out, "{}[key: string]: {t};", pad(indent + 1)).unwrap();
                }
                out.push_str(&pad(indent));
                out.push('}');
                out
            }
            Schema::Union {
                variants,
                discriminator,
                ..
            } => union(
                variants
                    .iter()
                    .map(|v| self.tag(self.ty(v, indent, view), v, discriminator.as_ref()))
                    .collect(),
            ),
            Schema::Intersection(parts) => parts
                .iter()
                .map(|p| wrap(p, self.ty(p, indent, view)))
                .collect::<Vec<_>>()
                .join(" & "),
        }
    }

    /// A union variant `v` rendered as `t`, tagged with its discriminating
    /// values when it is a reference, so that the union narrows on them.
    fn tag(&self, t: String, v: &Schema, d: Option<&Discriminator>) -> String {
        let (Some(d), Schema::Ref(k)) = (d, v) else {
            return t;
        };
        let property = key(&d.property_name);
        // The default variant takes every value the mapping does not list.
        if d.default_mapping.as_deref() == Some(k.as_str()) {
            return format!("({t} & {{ {property}: string }})");
        }
        let values: Vec<String> = d
            .mapping
            .iter()
            .filter(|(_, target)| target == k)
            .map(|(value, _)| quote(value))
            .collect();
        if values.is_empty() {
            return t;
        }
        format!("({t} & {{ {property}: {} }})", values.join(" | "))
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
    fn define(
        &self,
        s: &'a NamedSchema,
        view: View,
        edges: &mut Vec<(&'a str, &'a str)>,
    ) -> String {
        let mut d = Defining {
            key: &s.key,
            expanding: Vec::new(),
            edges: Vec::new(),
        };
        let parts = self.eager(&s.schema, view, &mut d, true, false);
        edges.append(&mut d.edges);
        join(parts)
    }

    /// The intersection members of `s` at the top of the alias of `d.key`.
    /// `member` says `s` is an `allOf` member (or the whole alias), where a
    /// reference to a parent listing `d.key` is replaced by the parent's own
    /// part; `inlined` says that is what is being rendered, so the listing
    /// itself is dropped.
    fn eager(
        &self,
        s: &'a Schema,
        view: View,
        d: &mut Defining<'a>,
        member: bool,
        inlined: bool,
    ) -> Vec<Part> {
        match s {
            Schema::Ref(k) if member && self.enumerates(k, d.key) => {
                if d.expanding.contains(&k.as_str()) {
                    return Vec::new();
                }
                d.expanding.push(k);
                let parts = self.eager(&self.schemas[k.as_str()].schema, view, d, true, true);
                d.expanding.pop();
                parts
            }
            Schema::Ref(k) => {
                d.edges.push((d.key, k));
                vec![Part {
                    text: self.reference(k, view),
                    compound: false,
                }]
            }
            Schema::Union {
                variants,
                discriminator,
                ..
            } => {
                if inlined && variants.iter().any(|v| self.reaches(v, d.key, true)) {
                    return Vec::new();
                }
                let types = variants
                    .iter()
                    .map(|v| {
                        let t = join(self.eager(v, view, d, false, inlined));
                        self.tag(t, v, discriminator.as_ref())
                    })
                    .collect();
                vec![Part {
                    text: union(types),
                    compound: compound(s),
                }]
            }
            Schema::Intersection(parts) => parts
                .iter()
                .flat_map(|p| self.eager(p, view, d, true, inlined))
                .collect(),
            _ => vec![Part {
                text: self.ty(s, 0, view),
                compound: compound(s),
            }],
        }
    }

    /// Whether schema `p` lists `c` among its subtypes: `c` is reachable from
    /// it through `$ref`, `allOf`, `oneOf` and `anyOf` alone, crossing at least
    /// one `oneOf` or `anyOf`.
    fn enumerates(&self, p: &str, c: &str) -> bool {
        self.reaches(&self.schemas[p].schema, c, false)
    }

    /// Whether `c` is reachable from `s` through `$ref`, `allOf`, `oneOf` and
    /// `anyOf` alone, crossing a `oneOf` or `anyOf` (`crossed` says one was
    /// crossed already).
    fn reaches(&self, s: &'a Schema, c: &str, crossed: bool) -> bool {
        let mut stack = vec![(s, crossed)];
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
    /// Parents whose own part is being rendered in place of a reference.
    expanding: Vec<&'a str>,
    /// References left in eager positions, as (from, to).
    edges: Vec<(&'a str, &'a str)>,
}

/// An intersection member of an alias.
struct Part {
    text: String,
    /// Needs parentheses next to `&`.
    compound: bool,
}

/// Intersection members as one type; none means no constraint.
fn join(mut parts: Vec<Part>) -> String {
    match parts.len() {
        0 => "unknown".into(),
        1 => parts.pop().unwrap().text,
        _ => parts
            .iter()
            .map(|p| {
                if p.compound {
                    format!("({})", p.text)
                } else {
                    p.text.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" & "),
    }
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
                    )));
                }
                Some(_) => {}
            }
        }
    }
    Ok(())
}

/// Visits a schema tree without following references.
fn walk<'s>(s: &'s Schema, f: &mut dyn FnMut(&'s Schema)) {
    f(s);
    match s {
        Schema::Array {
            prefix_items,
            items,
        } => {
            prefix_items.iter().for_each(|c| walk(c, f));
            if let Some(c) = items {
                walk(c, f);
            }
        }
        Schema::Object {
            properties,
            additional,
        } => {
            properties.iter().for_each(|p| walk(&p.schema, f));
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
fn default_media_type(body: &Body) -> Option<&str> {
    let declared: Vec<&str> = body
        .content
        .iter()
        .map(|c| c.media_type.as_str())
        .filter(|m| !m.contains('*'))
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
        } => {
            prefix_items.iter_mut().for_each(|c| walk_mut(c, f));
            if let Some(c) = items {
                walk_mut(c, f);
            }
        }
        Schema::Object {
            properties,
            additional,
        } => {
            properties
                .iter_mut()
                .for_each(|p| walk_mut(&mut p.schema, f));
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
fn security_literal(security: &[SecurityRequirement]) -> String {
    let items: Vec<String> = security
        .iter()
        .map(|r| {
            let schemes: Vec<String> = r
                .schemes
                .iter()
                .map(|(name, scopes)| {
                    let scopes: Vec<String> = scopes.iter().map(|s| quote(s)).collect();
                    format!("{}: [{}]", key(name), scopes.join(", "))
                })
                .collect();
            if schemes.is_empty() {
                "{}".to_string()
            } else {
                format!("{{ {} }}", schemes.join(", "))
            }
        })
        .collect();
    format!("[{}]", items.join(", "))
}

/// Non-default serialization of a parameter, as a runtime `ParamStyle` literal.
fn param_style(p: &Param) -> Option<String> {
    let mut fields = Vec::new();
    match &p.value {
        ParamValue::Content(c) => match p.location {
            Location::Query if c.kind() == MediaKind::Json => fields.push("style: \"json\"".into()),
            // Pre-serialized cookie values are sent as-is.
            Location::Cookie => fields.push("style: \"cookie\"".into()),
            _ => {}
        },
        ParamValue::Schema {
            style,
            explode,
            allow_reserved,
            ..
        } => {
            match style {
                Style::DeepObject => fields.push("style: \"deepObject\"".into()),
                Style::Cookie => fields.push("style: \"cookie\"".into()),
                Style::Form | Style::Simple => {}
            }
            let default_explode = *style != Style::Simple;
            if *explode != default_explode && *style != Style::DeepObject {
                fields.push(format!("explode: {explode}"));
            }
            if *allow_reserved && p.location == Location::Query {
                fields.push("reserved: true".into());
            }
        }
    }
    (!fields.is_empty()).then(|| format!("{{ {} }}", fields.join(", ")))
}

/// Parenthesizes compound types where they are used as an operand.
fn wrap(s: &Schema, t: String) -> String {
    if compound(s) { format!("({t})") } else { t }
}

/// Whether the type of `s` is a union or intersection at its top level, which
/// needs parentheses next to `&` or `[]`. A union of one variant renders as
/// that variant, so it is compound when the variant is.
fn compound(s: &Schema) -> bool {
    match s {
        Schema::Union { variants, .. } => variants.len() > 1 || variants.iter().any(compound),
        Schema::Enum { values, .. } => values.len() > 1,
        Schema::Intersection(_) => true,
        _ => false,
    }
}

fn union(types: Vec<String>) -> String {
    let mut seen = HashSet::new();
    let unique: Vec<String> = types
        .into_iter()
        .filter(|t| seen.insert(t.clone()))
        .collect();
    match unique.len() {
        0 => "never".into(),
        _ => unique.join(" | "),
    }
}

fn jsdoc(meta: &Meta, extra: &[String], indent: usize) -> String {
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
    if lines.is_empty() {
        return String::new();
    }
    // Every line may contain document text; `*/` would end the comment.
    let lines: Vec<String> = lines.iter().map(|l| l.replace("*/", "*\\/")).collect();
    let p = pad(indent);
    if lines.len() == 1 {
        return format!("{p}/** {} */\n", lines[0]);
    }
    let mut out = format!("{p}/**\n");
    for l in lines {
        if l.is_empty() {
            writeln!(out, "{p} *").unwrap();
        } else {
            writeln!(out, "{p} * {l}").unwrap();
        }
    }
    writeln!(out, "{p} */").unwrap();
    out
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

fn key(name: &str) -> String {
    let mut chars = name.chars();
    let ident = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
    if ident { name.to_string() } else { quote(name) }
}

fn quote(s: &str) -> String {
    literal(&Value::from(s))
}

/// A JSON value as a TypeScript literal. U+2028 and U+2029 are escaped too:
/// JSON allows them raw, but they end a `//` comment and TypeScript before
/// 5.3 rejects them in string literals.
fn literal(v: &Value) -> String {
    v.to_string()
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

fn opt(optional: bool) -> &'static str {
    if optional { "?" } else { "" }
}

fn pad(level: usize) -> String {
    "  ".repeat(level)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
            "type $Animal = {\n  kind: string;\n} & (($Dog & { kind: \"Dog\" }) | ($Cat & { kind: \"Cat\" }));\n",
        );
        has("type $Dog = {\n  kind: string;\n} & ($Puppy | $Adult);\n");
        has("type $Cat = {\n  kind: string;\n} & {\n  lives?: number;\n};\n");
        // Two levels down, Puppy gets Animal's part through Dog's, without either listing.
        has("type $Puppy = {\n  kind: string;\n} & {\n  age?: number;\n};\n");
        has("type $Adult = {\n  kind: string;\n};\n");
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
