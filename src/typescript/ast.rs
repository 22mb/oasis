//! The TypeScript the emitter writes, as a tree, and its printer. The emitter
//! builds the tree from the IR; indentation, parentheses, quoting and comments
//! are decided here alone.

use serde_json::Value;
use std::collections::HashSet;

pub enum Type {
    /// A keyword or a named type: `unknown`, `string`, `$Pet`, `Blob`.
    Name(String),
    /// A string, number, boolean or `null` literal.
    Literal(Value),
    /// `Name<A, B>`.
    Generic(&'static str, Vec<Type>),
    /// `T["key"]`.
    Index(Box<Type>, String),
    Array(Box<Type>),
    /// `[A, B]`, then `...R[]` for the rest.
    Tuple(Vec<Type>, Option<Box<Type>>),
    /// An object type, on one line (`{ k: V }`) or one member per line.
    Object {
        members: Vec<Member>,
        multiline: bool,
    },
    /// Without duplicates; `never` when empty.
    Union(Vec<Type>),
    /// `unknown` when empty.
    Intersection(Vec<Type>),
    /// `(T)`, where the parentheses are not needed but help the reader.
    Paren(Box<Type>),
}

impl Type {
    pub fn name(name: impl Into<String>) -> Type {
        Type::Name(name.into())
    }

    pub fn string(s: &str) -> Type {
        Type::Literal(Value::from(s))
    }

    pub fn object(members: Vec<Member>) -> Type {
        Type::Object {
            members,
            multiline: true,
        }
    }

    pub fn inline(members: Vec<Member>) -> Type {
        Type::Object {
            members,
            multiline: false,
        }
    }

    pub fn is(&self, name: &str) -> bool {
        matches!(self, Type::Name(n) if n == name)
    }

    /// Whether this type refers to the type named `name`.
    pub fn refers_to(&self, name: &str) -> bool {
        match self {
            Type::Name(n) => n == name,
            Type::Literal(_) => false,
            Type::Generic(_, ts) | Type::Union(ts) | Type::Intersection(ts) => {
                ts.iter().any(|t| t.refers_to(name))
            }
            Type::Index(t, _) | Type::Array(t) | Type::Paren(t) => t.refers_to(name),
            Type::Tuple(ts, rest) => ts.iter().chain(rest.as_deref()).any(|t| t.refers_to(name)),
            Type::Object { members, .. } => members.iter().any(|m| m.ty.refers_to(name)),
        }
    }

    /// Writes the type, on a line at nesting level `indent`, and says whether
    /// it is a union or intersection at the top, which an operand parenthesizes.
    fn write(&self, out: &mut String, indent: usize) -> bool {
        match self {
            Type::Name(n) => out.push_str(n),
            Type::Literal(v) => out.push_str(&literal(v)),
            Type::Generic(name, args) => {
                out.push_str(name);
                out.push('<');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    a.write(out, indent);
                }
                out.push('>');
            }
            Type::Index(t, key) => {
                t.operand(out, indent);
                out.push('[');
                out.push_str(&quote(key));
                out.push(']');
            }
            Type::Array(t) => {
                t.operand(out, indent);
                out.push_str("[]");
            }
            Type::Tuple(ts, rest) => {
                out.push('[');
                for (i, t) in ts.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    t.write(out, indent);
                }
                if let Some(r) = rest {
                    if !ts.is_empty() {
                        out.push_str(", ");
                    }
                    out.push_str("...");
                    r.operand(out, indent);
                    out.push_str("[]");
                }
                out.push(']');
            }
            Type::Object { members, .. } if members.is_empty() => out.push_str("{}"),
            Type::Object {
                members,
                multiline: true,
            } => {
                out.push_str("{\n");
                Member::lines(members, out, indent + 1);
                pad(out, indent);
                out.push('}');
            }
            Type::Object { members, .. } => {
                out.push_str("{ ");
                for (i, m) in members.iter().enumerate() {
                    if i > 0 {
                        out.push_str("; ");
                    }
                    // A member's own lines are indented as if it began a line.
                    m.write(out, indent + 1);
                }
                out.push_str(" }");
            }
            // `&` binds tighter than `|`, so no member needs parentheses.
            // Duplicates are found by their text.
            Type::Union(ts) => {
                let mut seen = HashSet::new();
                let mut types = Vec::new();
                for t in ts {
                    let mut s = String::new();
                    let compound = t.write(&mut s, indent);
                    if seen.insert(s.clone()) {
                        types.push((s, compound));
                    }
                }
                match types.as_slice() {
                    [] => out.push_str("never"),
                    [(s, compound)] => {
                        out.push_str(s);
                        return *compound;
                    }
                    _ => {
                        for (i, (s, _)) in types.iter().enumerate() {
                            if i > 0 {
                                out.push_str(" | ");
                            }
                            out.push_str(s);
                        }
                        return true;
                    }
                }
            }
            Type::Intersection(ts) => match ts.as_slice() {
                [] => out.push_str("unknown"),
                [t] => return t.write(out, indent),
                _ => {
                    for (i, t) in ts.iter().enumerate() {
                        if i > 0 {
                            out.push_str(" & ");
                        }
                        t.operand(out, indent);
                    }
                    return true;
                }
            },
            Type::Paren(t) => {
                out.push('(');
                t.write(out, indent);
                out.push(')');
            }
        }
        false
    }

    /// Writes the type as an operand of `&`, `[]` or `[key]`, parenthesized if
    /// it is a union or intersection.
    fn operand(&self, out: &mut String, indent: usize) {
        let start = out.len();
        if self.write(out, indent) {
            out.insert(start, '(');
            out.push(')');
        }
    }
}

/// A member of an object type or interface.
pub struct Member {
    pub doc: Vec<String>,
    pub key: Key,
    pub optional: bool,
    pub ty: Type,
}

impl Member {
    pub fn new(key: Key, optional: bool, ty: Type) -> Member {
        Member {
            doc: Vec::new(),
            key,
            optional,
            ty,
        }
    }

    pub fn doc(mut self, doc: Vec<String>) -> Member {
        self.doc = doc;
        self
    }

    /// `key?: T`, without the `;`.
    fn write(&self, out: &mut String, indent: usize) {
        match &self.key {
            Key::Name(k) => out.push_str(&key(k)),
            Key::Quoted(k) => out.push_str(&quote(k)),
            Key::Index => out.push_str("[key: string]"),
        }
        if self.optional {
            out.push('?');
        }
        out.push_str(": ");
        self.ty.write(out, indent);
    }

    /// One member per line at level `indent`, each with its comment.
    fn lines(members: &[Member], out: &mut String, indent: usize) {
        for m in members {
            jsdoc(out, &m.doc, indent);
            pad(out, indent);
            m.write(out, indent);
            out.push_str(";\n");
        }
    }
}

pub enum Key {
    /// Quoted unless it is an identifier.
    Name(String),
    Quoted(String),
    /// `[key: string]`.
    Index,
}

impl Key {
    pub fn name(k: impl Into<String>) -> Key {
        Key::Name(k.into())
    }
}

/// A value: the runtime data the file passes to the client.
pub enum Expr {
    Literal(Value),
    Array(Vec<Expr>),
    /// An object, on one line or one entry per line; keys are quoted unless
    /// they are identifiers.
    Object {
        entries: Vec<(String, Expr)>,
        multiline: bool,
    },
}

impl Expr {
    pub fn string(s: &str) -> Expr {
        Expr::Literal(Value::from(s))
    }

    pub fn object(entries: Vec<(String, Expr)>) -> Expr {
        Expr::Object {
            entries,
            multiline: true,
        }
    }

    pub fn inline(entries: Vec<(String, Expr)>) -> Expr {
        Expr::Object {
            entries,
            multiline: false,
        }
    }

    fn write(&self, out: &mut String, indent: usize) {
        match self {
            Expr::Literal(v) => out.push_str(&literal(v)),
            Expr::Array(items) => {
                out.push('[');
                for (i, e) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    e.write(out, indent);
                }
                out.push(']');
            }
            Expr::Object { entries, .. } if entries.is_empty() => out.push_str("{}"),
            Expr::Object {
                entries,
                multiline: true,
            } => {
                out.push_str("{\n");
                for (k, v) in entries {
                    pad(out, indent + 1);
                    out.push_str(&key(k));
                    out.push_str(": ");
                    v.write(out, indent + 1);
                    out.push_str(",\n");
                }
                pad(out, indent);
                out.push('}');
            }
            Expr::Object { entries, .. } => {
                out.push_str("{ ");
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&key(k));
                    out.push_str(": ");
                    v.write(out, indent);
                }
                out.push_str(" }");
            }
        }
    }
}

/// A top-level statement, printed on its own lines.
pub enum Item {
    /// `//` comment lines.
    Comment(Vec<String>),
    Blank,
    /// `type Name = T;`, exported or not, with its doc comment.
    Alias {
        doc: Vec<String>,
        export: bool,
        name: String,
        ty: Type,
    },
    /// `export interface Name { ... }`.
    Interface {
        name: String,
        members: Vec<Member>,
    },
    /// `const name: T = value;`, or `as const` without a type.
    Const {
        name: String,
        ty: Option<Type>,
        value: Expr,
    },
    /// `export type { local as name, ... };`
    ExportTypes(Vec<(String, String)>),
    /// `export { local as name, ... };`
    Export(Vec<(String, String)>),
    /// Text written as it is.
    Raw(String),
}

impl Item {
    pub fn alias(name: impl Into<String>, ty: Type) -> Item {
        Item::Alias {
            doc: Vec::new(),
            export: false,
            name: name.into(),
            ty,
        }
    }

    fn write(&self, out: &mut String) {
        let specifiers = |out: &mut String, names: &[(String, String)]| {
            for (local, name) in names {
                out.push_str(&format!("  {local} as {name},\n"));
            }
            out.push_str("};\n");
        };
        match self {
            Item::Comment(lines) => {
                for l in lines {
                    out.push_str(&format!("// {l}\n"));
                }
            }
            Item::Blank => out.push('\n'),
            Item::Alias {
                doc,
                export,
                name,
                ty,
            } => {
                jsdoc(out, doc, 0);
                if *export {
                    out.push_str("export ");
                }
                out.push_str(&format!("type {name} = "));
                ty.write(out, 0);
                out.push_str(";\n");
            }
            Item::Interface { name, members } => {
                out.push_str(&format!("export interface {name} {{\n"));
                Member::lines(members, out, 1);
                out.push_str("}\n");
            }
            Item::Const { name, ty, value } => {
                out.push_str(&format!("const {name}"));
                if let Some(t) = ty {
                    out.push_str(": ");
                    t.write(out, 0);
                }
                out.push_str(" = ");
                value.write(out, 0);
                out.push_str(if ty.is_some() { ";\n" } else { " as const;\n" });
            }
            Item::ExportTypes(names) => {
                out.push_str("export type {\n");
                specifiers(out, names);
            }
            Item::Export(names) => {
                out.push_str("export {\n");
                specifiers(out, names);
            }
            Item::Raw(text) => out.push_str(text),
        }
    }
}

pub fn print(items: &[Item]) -> String {
    let mut out = String::new();
    for item in items {
        item.write(&mut out);
    }
    out
}

/// Writes a doc comment of `lines` at level `indent`, if there are any.
fn jsdoc(out: &mut String, lines: &[String], indent: usize) {
    // Every line may contain document text; `*/` would end the comment.
    let escape = |l: &str| l.replace("*/", "*\\/");
    match lines {
        [] => {}
        [line] => {
            pad(out, indent);
            out.push_str(&format!("/** {} */\n", escape(line)));
        }
        _ => {
            pad(out, indent);
            out.push_str("/**\n");
            for l in lines {
                pad(out, indent);
                if l.is_empty() {
                    out.push_str(" *\n");
                } else {
                    out.push_str(&format!(" * {}\n", escape(l)));
                }
            }
            pad(out, indent);
            out.push_str(" */\n");
        }
    }
}

/// `name` as an object key: quoted unless it is an identifier.
fn key(name: &str) -> String {
    let mut chars = name.chars();
    let ident = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
    if ident { name.to_string() } else { quote(name) }
}

pub fn quote(s: &str) -> String {
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

fn pad(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("  ");
    }
}
