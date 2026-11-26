//! `crates/opencode-plugin-spec/docs/api.md`: the reference manual, for people and models alike.

use super::model::{Contract, HALVES, Member, MemberKind, TypeDef, TypeKind, context_type};
use super::sig;
use super::{GENERATED, Out};

pub fn api(c: &Contract) -> String {
    let mut o = Out::default();
    o.line(&format!("# Brocode plugin API reference (contract {})", c.contract));
    o.blank();
    o.line(&format!("> {GENERATED} For a walkthrough, read [Writing a plugin](writing-a-plugin.md)."));
    o.blank();
    o.line("Contents: [Versioning](#versioning) · [Modules and globals](#modules-and-globals) · [Contexts](#contexts) · [Contributions](#contributions) · [Services](#services) · [UI nodes](#ui-nodes) · [Semantic tokens](#semantic-tokens) · [Enums](#enums) · [Types](#types) · [plugin.toml](#plugintoml)");
    o.blank();

    o.line("## Versioning");
    o.blank();
    o.line(&format!("Contract versions are `{}`. This is contract `{}`.", c.versioning.format, c.contract));
    o.blank();
    for r in &c.versioning.rules {
        o.line(&format!("- {r}"));
    }
    o.blank();

    o.line("## Modules and globals");
    o.blank();
    o.line("### `require(\"@opencode/plugin\")`");
    o.blank();
    members(&mut o, &c.plugin_module);
    o.line("### `require(\"@opencode/ui\")`");
    o.blank();
    o.line("Every [UI node](#ui-nodes) constructor, and:");
    o.blank();
    members(&mut o, &c.ui_extras);
    o.line("The module also exports the `Ui*` types without their prefix: `ui.Node`, `ui.BoxProps`, `ui.Event`. Every other contract type is a global type.");
    o.blank();
    for g in &c.globals {
        o.line(&format!("### `{}`", g.name));
        o.blank();
        o.line(&g.doc);
        o.blank();
        if g.kind == MemberKind::Library {
            members(&mut o, &g.members);
        } else {
            o.line(&format!("`{}{}`", g.name, cell(g.sig.as_deref().unwrap_or(""))));
            o.blank();
        }
    }

    o.line("## Contexts");
    o.blank();
    o.line(&c.context.doc);
    o.blank();
    for half in HALVES {
        o.line(&format!("### `{}`", context_type(half)));
        o.blank();
        o.line("| Member | Type | Description |");
        o.line("|---|---|---|");
        for m in c.context.members.iter().filter(|m| m.halves.iter().any(|h| h == half)) {
            o.line(&format!("| `{}` | {} | {} |", m.name, member_type(m), cell(&m.doc)));
        }
        let services: Vec<String> = c.services_for(half).map(|s| format!("[`{}`](#{})", s.name, s.name.to_lowercase())).collect();
        o.line(&format!("| `GetService` | `(name) -> <Name>Service` | One of: {}. |", services.join(", ")));
        for k in c.contributions_for(half) {
            o.line(&format!("| `{}` | `(spec: {}) -> {}` | {} |", k.method, k.spec, k.returns(), cell(&k.doc)));
        }
        for e in c.events.iter().filter(|e| e.halves.iter().any(|h| h == half)) {
            o.line(&format!("| `{}` | `Signal<{}>` | {} |", e.name, cell(&e.payload), cell(&e.doc)));
        }
        o.blank();
    }

    o.line("## Contributions");
    o.blank();
    o.line("Every `Add*` method returns a handle the host withdraws with the plugin half. Callbacks run on the UI thread; render callbacks (`render`, `label`, `show`, `match`) must not yield, and the rest run as coroutines that may.");
    o.blank();
    o.line("| Kind | Method | Halves | Spec | Desktop's name |");
    o.line("|---|---|---|---|---|");
    for k in &c.contributions {
        let official = c.type_def(&k.spec).and_then(|t| t.official.as_deref()).unwrap_or("—");
        o.line(&format!("| `{}` | `{}` | {} | [`{}`](#{}) | {official}{} |", k.kind, k.method, k.halves.join(", "), k.spec, k.spec.to_lowercase(), note(&k.since, &None)));
    }
    o.blank();

    o.line("## Services");
    o.blank();
    o.line("Get one with `ctx:GetService(\"Name\")`. Methods named `…Async` yield: call them from a handler, a task or a signal callback, never from a render.");
    o.blank();
    for s in &c.services {
        o.line(&format!("### {}", s.name));
        o.blank();
        o.line(&format!("{} Halves: {}.{}", s.doc, s.halves.join(", "), note(&s.since, &None)));
        o.blank();
        members(&mut o, &s.members);
    }

    o.line("## UI nodes");
    o.blank();
    o.line("`ui.*` constructors build host elements straight into the frame's arena. Wrap subtrees in `view:Memo` so unchanged frames call no Luau at all.");
    o.blank();
    o.line("| Constructor | Category | Props | Description |");
    o.line("|---|---|---|---|");
    for n in &c.nodes {
        o.line(&format!("| `ui.{}{}` | {} | [`{}`](#{}) | {} |", n.name, cell(&sig::luau(&n.sig)), n.category, n.props, n.props.to_lowercase(), cell(&format!("{}{}", n.doc, note(&n.since, &None)))));
    }
    o.blank();
    o.line("### Native nodes");
    o.blank();
    o.line("Heavy engines stay native in the host. Luau composes them as opaque nodes and drives them through services.");
    o.blank();
    o.line("| Node | Engine | Stays native | Moves to Luau |");
    o.line("|---|---|---|---|");
    for n in c.nodes.iter().filter(|n| n.native.is_some()) {
        if let Some(native) = &n.native {
            o.line(&format!("| `ui.{}` | {} | {} | {} |", n.name, cell(&native.engine), cell(&native.native), cell(&native.luau)));
        }
    }
    o.blank();

    o.line("## Semantic tokens");
    o.blank();
    o.line("A `Color` is one of these tokens, or `ui.hex(\"#RRGGBB\")`. The host resolves tokens for the current theme and color scheme.");
    o.blank();
    o.line("| Token | Group | Meaning |");
    o.line("|---|---|---|");
    for t in &c.tokens {
        let fallback = t.fallback.as_ref().map(|f| format!(" Falls back to `{f}` on older hosts.")).unwrap_or_default();
        o.line(&format!("| `{}` | {} | {}{fallback}{} |", t.name, t.group, cell(&t.doc), note(&t.since, &t.deprecated)));
    }
    o.blank();

    o.line("## Enums");
    o.blank();
    for e in &c.enums {
        o.line(&format!("### {}", e.name));
        o.blank();
        o.line(&e.doc);
        o.blank();
        o.line("| Value | Meaning |");
        o.line("|---|---|");
        for v in &e.values {
            o.line(&format!("| `\"{}\"` | {}{} |", v.value, cell(&v.doc), note(&v.since, &v.deprecated)));
        }
        o.blank();
    }

    o.line("## Types");
    o.blank();
    for t in c.types.iter().filter(|t| t.kind != TypeKind::Tokens) {
        type_section(&mut o, t);
    }

    o.line("## plugin.toml");
    o.blank();
    o.line(&format!("{} Editors validate it with `plugins/plugin.schema.json`: add `#:schema ../plugin.schema.json` as its first line.", c.manifest.doc));
    o.blank();
    o.line("| Key | Type | Required | Description |");
    o.line("|---|---|---|---|");
    for f in &c.manifest.fields {
        manifest_row(&mut o, "", f);
        for sub in &f.fields {
            manifest_row(&mut o, &format!("{}.", f.name), sub);
        }
    }
    o.blank();
    o.finish()
}

fn manifest_row(o: &mut Out, prefix: &str, f: &super::model::ManifestField) {
    let ty = match (&f.enum_name, f.ty.as_str()) {
        (Some(e), "enum[]") => format!("array of [`{e}`](#{})", e.to_lowercase()),
        (Some(e), _) => format!("[`{e}`](#{})", e.to_lowercase()),
        (None, t) => format!("`{t}`"),
    };
    let default = f.default.as_ref().map(|d| format!(" Defaults to `\"{d}\"`.")).unwrap_or_default();
    o.line(&format!("| `{prefix}{}` | {ty} | {} | {}{default} |", f.name, if f.required { "yes" } else { "no" }, cell(&f.doc)));
}

fn type_section(o: &mut Out, t: &TypeDef) {
    o.line(&format!("### {}", t.name));
    o.blank();
    let official = t.official.as_ref().map(|n| format!(" Desktop's plugin API calls it `{n}`.")).unwrap_or_default();
    let generics = t.generics.as_ref().map(|g| format!(" Written `{}<{g}>`.", t.name)).unwrap_or_default();
    o.line(&format!("{}{official}{generics}", t.doc));
    o.blank();
    match t.kind {
        TypeKind::Alias => {
            o.line(&format!("`type {} = {}`", t.name, sig::luau(t.ty.as_deref().unwrap_or(""))));
            o.blank();
        }
        TypeKind::Record => {
            if !t.extends.is_empty() {
                let bases: Vec<String> = t.extends.iter().map(|b| format!("[`{b}`](#{})", b.to_lowercase())).collect();
                o.line(&format!("Also takes every field of {}.", bases.join(", ")));
                o.blank();
            }
            if t.fields.is_empty() {
                return;
            }
            o.line("| Field | Type | Description |");
            o.line("|---|---|---|");
            for f in &t.fields {
                o.line(&format!("| `{}` | `{}` | {}{} |", f.name, cell(&sig::luau(&f.ty)), cell(&f.doc), note(&f.since, &f.deprecated)));
            }
            o.blank();
        }
        TypeKind::Object => members(o, &t.members),
        TypeKind::Opaque => {
            o.line("Opaque: the host owns it, and Luau sees no fields.");
            o.blank();
        }
        TypeKind::Tokens => {}
    }
}

fn members(o: &mut Out, list: &[Member]) {
    o.line("| Member | Type | Description |");
    o.line("|---|---|---|");
    for m in list {
        let yields = if m.yields { " Yields." } else { "" };
        let deprecated = note(&m.since, &m.deprecated);
        let call = if m.kind == MemberKind::Method { ":" } else { "." };
        o.line(&format!("| `{call}{}` | {} | {}{yields}{deprecated} |", m.name, member_type(m), cell(&m.doc)));
    }
    o.blank();
}

fn member_type(m: &Member) -> String {
    let generics = m.generics.as_ref().map(|g| format!("<{g}>")).unwrap_or_default();
    match m.kind {
        MemberKind::Field => format!("`{}`", cell(&sig::luau(m.ty.as_deref().unwrap_or("any")))),
        MemberKind::Signal => format!("`Signal<{}>`", cell(m.payload.as_deref().unwrap_or("()"))),
        _ => format!("`{generics}{}`", cell(&sig::luau(m.sig.as_deref().unwrap_or("")))),
    }
}

/// "Since 1.2." and "Deprecated: …" notes.
fn note(since: &Option<String>, deprecated: &Option<String>) -> String {
    let since = since.as_ref().map(|v| format!(" Since {v}.")).unwrap_or_default();
    let deprecated = deprecated.as_ref().map(|r| format!(" **Deprecated:** {r}")).unwrap_or_default();
    format!("{since}{deprecated}")
}

/// Text safe inside a Markdown table cell.
fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}
