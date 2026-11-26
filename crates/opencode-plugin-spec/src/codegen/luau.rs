//! `plugins/opencode.d.luau` (every contract type as a global type, with `GetService` overloads),
//! the `@opencode/ui` and `@opencode/plugin` module stubs luau-lsp resolves through `.luaurc`,
//! and `.luaurc` itself.

use super::model::{Contract, HALVES, Member, MemberKind, TypeDef, TypeKind, context_type};
use super::sig::{self, parse_sig};
use super::{GENERATED, Out};

pub fn definitions(c: &Contract) -> Result<String, String> {
    let mut o = Out::default();
    o.line(&format!("-- Brocode plugin contract {}: global types for luau-lsp.", c.contract));
    o.line(&format!("-- {GENERATED}"));
    o.line("-- Load it with `luau-lsp analyze --definitions=@opencode=plugins/opencode.d.luau`.");
    o.blank();

    o.section("Semantic tokens");
    doc(&mut o, 0, &c.type_def("SemanticToken").map(|t| t.doc.clone()).unwrap_or_default(), None);
    o.line("export type SemanticToken =");
    for (i, t) in c.tokens.iter().enumerate() {
        let lead = if i == 0 { "    " } else { "    | " };
        let since = t.since.as_ref().map(|v| format!(" Since {v}.")).unwrap_or_default();
        let deprecated = t.deprecated.as_ref().map(|r| format!(" @deprecated {r}")).unwrap_or_default();
        o.line(&format!("{lead}\"{}\" -- {}{since}{deprecated}", t.name, t.doc));
    }
    o.blank();

    o.section("Enums");
    for e in &c.enums {
        doc(&mut o, 0, &e.doc, None);
        let values: Vec<String> = e.values.iter().map(|v| format!("\"{}\"", v.value)).collect();
        o.line(&format!("export type {} = {}", e.name, values.join(" | ")));
        o.blank();
    }

    o.section("Opaque host types");
    for t in c.types.iter().filter(|t| t.kind == TypeKind::Opaque) {
        doc(&mut o, 0, &t.doc, None);
        o.line(&format!("declare extern type {} with end", t.name));
        o.blank();
    }

    o.section("Aliases, records and host objects");
    for t in &c.types {
        match t.kind {
            TypeKind::Alias => {
                doc(&mut o, 0, &t.doc, None);
                o.line(&format!("export type {} = {}", t.name, sig::luau(t.ty.as_deref().unwrap_or("any"))));
                o.blank();
            }
            TypeKind::Record => record(&mut o, c, t),
            TypeKind::Object => object(&mut o, &t.name, t.generics.as_deref(), &t.doc, &t.members)?,
            TypeKind::Tokens | TypeKind::Opaque => {}
        }
    }

    o.section("Services");
    for s in &c.services {
        let halves = if s.halves.len() == 2 { "Both halves".to_owned() } else { format!("The {} half only", s.halves.join(", ")) };
        object(&mut o, &s.type_name(), None, &format!("{} {halves}: `ctx:GetService(\"{}\")`.", s.doc, s.name), &s.members)?;
    }

    o.section("Plugin contexts");
    for half in HALVES {
        context(&mut o, c, half)?;
    }

    o.section("Modules");
    doc(&mut o, 0, "`require(\"@opencode/ui\")`: declarative node constructors. Nodes build host elements directly; there is no virtual DOM.", None);
    o.line("export type UiLibrary = {");
    for n in &c.nodes {
        let native = if n.native.is_some() { " Native: the host renders it." } else { "" };
        doc(&mut o, 1, &format!("{}{native}", n.doc), None);
        o.line(&format!("    {}: {},", n.name, sig::luau(&n.sig)));
    }
    for m in &c.ui_extras {
        member(&mut o, "UiLibrary", m)?;
    }
    o.line("}");
    o.blank();
    doc(&mut o, 0, "`require(\"@opencode/plugin\")`: defines a plugin's halves.", None);
    o.line("export type PluginLibrary = {");
    for m in &c.plugin_module {
        member(&mut o, "PluginLibrary", m)?;
    }
    o.line("}");
    o.blank();

    o.section("Globals");
    for g in &c.globals {
        doc(&mut o, 0, &g.doc, None);
        match g.kind {
            MemberKind::Library => {
                o.line(&format!("declare {}: {{", g.name));
                for m in &g.members {
                    member(&mut o, &g.name, m)?;
                }
                o.line("}");
            }
            _ => {
                let s = parse_sig(g.sig.as_deref().unwrap_or("() -> ()"))?;
                let params: Vec<String> = s
                    .params
                    .iter()
                    .map(|p| match (&p.name, p.ty.strip_prefix("...")) {
                        (None, Some(rest)) => format!("...: {rest}"),
                        (Some(n), _) => format!("{n}: {}", sig::luau(&p.ty)),
                        (None, None) => sig::luau(&p.ty),
                    })
                    .collect();
                o.line(&format!("declare function {}({}): {}", g.name, params.join(", "), sig::luau(&s.ret)));
            }
        }
        o.blank();
    }
    Ok(o.finish())
}

fn record(o: &mut Out, c: &Contract, t: &TypeDef) {
    let official = t.official.as_ref().map(|n| format!(" Desktop's plugin API calls it `{n}`.")).unwrap_or_default();
    doc(o, 0, &format!("{}{official}", t.doc), None);
    o.line(&format!("export type {} = {{", t.name));
    for f in c.fields_of(t) {
        doc(o, 1, &f.doc, f.deprecated.as_deref());
        o.line(&format!("    {}: {},", f.name, sig::luau(&f.ty)));
    }
    o.line("}");
    o.blank();
}

fn object(o: &mut Out, name: &str, generics: Option<&str>, docs: &str, members: &[Member]) -> Result<(), String> {
    let full = generics.map(|g| format!("{name}<{g}>")).unwrap_or_else(|| name.to_owned());
    doc(o, 0, docs, None);
    o.line(&format!("export type {full} = {{"));
    for m in members {
        member(o, &full, m)?;
    }
    o.line("}");
    o.blank();
    Ok(())
}

/// A context is an extern type: Luau's new solver reads more than eight `GetService` overloads
/// only as extern methods. Extern methods cannot be generic, so a generic member becomes a
/// property typed by an alias declared ahead of it.
fn context(o: &mut Out, c: &Contract, half: &str) -> Result<(), String> {
    let name = context_type(half);
    let other = if half == "app" { "window" } else { "app" };
    let members: Vec<&Member> = c.context.members.iter().filter(|m| m.halves.is_empty() || m.halves.iter().any(|h| h == half)).collect();
    for m in members.iter().filter(|m| m.generics.is_some()) {
        o.line(&format!("export type {name}{} = {}", m.name, method_type(name, m)?));
    }
    o.blank();
    doc(o, 0, &format!("{} The {half} half's: see `plugin.define{}`.", c.context.doc, if half == "app" { "App" } else { "Window" }), None);
    o.line(&format!("declare extern type {name} with"));
    for m in members {
        doc(o, 1, &m.doc, m.deprecated.as_deref());
        match (m.kind, &m.generics) {
            (MemberKind::Method, Some(_)) => o.line(&format!("    {}: {name}{}", m.name, m.name)),
            (MemberKind::Method, None) => o.line(&format!("    {}", extern_method(&m.name, m.sig.as_deref().unwrap_or("() -> ()"))?)),
            (MemberKind::Signal, _) => o.line(&format!("    {}: Signal<{}>", m.name, sig::luau(m.payload.as_deref().unwrap_or("()")))),
            _ => o.line(&format!("    {}: {}", m.name, sig::luau(m.ty.as_deref().unwrap_or("any")))),
        }
    }
    let missing: Vec<&str> = c.services.iter().filter(|s| !s.halves.iter().any(|h| h == half)).map(|s| s.name.as_str()).collect();
    let note = if missing.is_empty() { String::new() } else { format!(" The {other} half alone has: {}.", missing.join(", ")) };
    for s in c.services_for(half) {
        doc(o, 1, &format!("Returns a host service by name; each overload narrows the type.{note}"), None);
        o.line(&format!("    function GetService(self, name: \"{}\"): {}", s.name, s.type_name()));
    }
    for k in c.contributions_for(half) {
        doc(o, 1, &k.doc, None);
        o.line(&format!("    function {}(self, spec: {}): {}", k.method, k.spec, k.returns()));
    }
    for e in c.events.iter().filter(|e| e.halves.iter().any(|h| h == half)) {
        doc(o, 1, &e.doc, None);
        o.line(&format!("    {}: Signal<{}>", e.name, sig::luau(&e.payload)));
    }
    o.line("end");
    o.blank();
    Ok(())
}

/// `function Name(self, a: A): R` inside an extern type.
fn extern_method(name: &str, signature: &str) -> Result<String, String> {
    let s = parse_sig(signature)?;
    let mut params = vec!["self".to_owned()];
    params.extend(s.params.iter().map(|p| match (&p.name, p.ty.strip_prefix("...")) {
        (Some(n), _) => format!("{n}: {}", sig::luau(&p.ty)),
        (None, Some(rest)) => format!("...: {}", sig::luau(rest)),
        (None, None) => sig::luau(&p.ty),
    }));
    Ok(format!("function {name}({}): {}", params.join(", "), sig::luau(&s.ret)))
}

/// A method's function type with `self` first: `<T>(self: Owner, a: A) -> R`.
fn method_type(owner: &str, m: &Member) -> Result<String, String> {
    let s = parse_sig(m.sig.as_deref().unwrap_or("() -> ()"))?;
    let generics = m.generics.as_ref().map(|g| format!("<{g}>")).unwrap_or_default();
    let mut params = vec![format!("self: {owner}")];
    params.extend(s.params.iter().map(|p| p.name.as_ref().map(|n| format!("{n}: {}", sig::luau(&p.ty))).unwrap_or_else(|| sig::luau(&p.ty))));
    Ok(format!("{generics}({}) -> {}", params.join(", "), sig::luau(&s.ret)))
}

fn member(o: &mut Out, owner: &str, m: &Member) -> Result<(), String> {
    let docs = if m.yields { format!("{} Yields: call it from a handler, a task or a signal callback.", m.doc) } else { m.doc.clone() };
    doc(o, 1, &docs, m.deprecated.as_deref());
    let generics = m.generics.as_ref().map(|g| format!("<{g}>")).unwrap_or_default();
    let line = match m.kind {
        MemberKind::Field => format!("{}: {},", m.name, sig::luau(m.ty.as_deref().unwrap_or("any"))),
        MemberKind::Signal => format!("{}: Signal<{}>,", m.name, sig::luau(m.payload.as_deref().unwrap_or("()"))),
        MemberKind::Method => format!("{}: {},", m.name, method_type(owner, m)?),
        MemberKind::Function | MemberKind::Library => format!("{}: {generics}{},", m.name, sig::luau(m.sig.as_deref().unwrap_or("() -> ()"))),
    };
    o.line(&format!("    {line}"));
    Ok(())
}

fn doc(o: &mut Out, indent: usize, text: &str, deprecated: Option<&str>) {
    let pad = "    ".repeat(indent);
    for line in text.lines() {
        o.line(&format!("{pad}--- {line}"));
    }
    if let Some(reason) = deprecated {
        o.line(&format!("{pad}--- @deprecated {reason}"));
    }
}

/// `plugins/types/ui.luau`: re-exports the `Ui*` types without their prefix (`ui.Node`).
pub fn ui_stub(c: &Contract) -> String {
    let mut o = stub_head("@opencode/ui");
    o.line("export type Token = SemanticToken");
    for name in c.types.iter().map(|t| &t.name).chain(c.enums.iter().map(|e| &e.name)) {
        if let Some(short) = name.strip_prefix("Ui").filter(|s| !s.is_empty()) {
            o.line(&format!("export type {short} = {name}"));
        }
    }
    o.blank();
    o.line("local ui: UiLibrary = nil :: any");
    o.line("return ui");
    o.finish()
}

/// `plugins/types/plugin.luau`.
pub fn plugin_stub() -> String {
    let mut o = stub_head("@opencode/plugin");
    o.line("local plugin: PluginLibrary = nil :: any");
    o.line("return plugin");
    o.finish()
}

fn stub_head(module: &str) -> Out {
    let mut o = Out::default();
    o.line("--!strict");
    o.line(&format!("-- {GENERATED}"));
    o.line(&format!("-- The host provides `{module}`; this file only gives luau-lsp its types. Contract types are global"));
    o.line("-- (see opencode.d.luau).");
    o.blank();
    o
}

pub fn luaurc() -> String {
    "{\n  \"languageMode\": \"strict\",\n  \"aliases\": {\n    \"opencode\": \"./types\"\n  }\n}\n".to_owned()
}
