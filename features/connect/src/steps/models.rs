//! Searchable model picker shown after connecting a provider
//! (packages/app/src/providers/connect/connected-models.tsx, opencode#66608).
//!
//! Reuses the model picker's SearchList presentation (opencode_ui::popover::LIST) scoped strictly
//! to the models the new connection brought: single-provider connections omit section headers;
//! multi-provider connections (such as Console workspaces) show sticky provider headers. Choosing
//! a model by click or Enter immediately confirms it and closes the dialog.

use opencode_kit::model::Model;
use opencode_ui::palette::Pal;
use opencode_ui::popover::{self as pop, LIST};
use opencode_ui::style::UI;
use rune::{El, Key, KeyEvent, Span, Transition, Tx, col, el, rich, row, text};

use super::connection_groups;
use crate::catalog::ProviderInfo;
use crate::dialog::{CONSOLE, Event};
use crate::{ConnectHost, Open};

/// search-list-empty height matching DialogSelectModel (120 pt).
const EMPTY_H: f32 = 120.0;

struct ModelRow {
    provider: ProviderInfo,
    model: Model,
}

fn confirm_model(host: &mut dyn ConnectHost, key: &str, tx: &mut Tx) {
    tx.instant();
    crate::connect_event(host, Event::ModelConfirmed(key.to_owned()), tx);
}

fn clear_active(host: &mut dyn ConnectHost, tx: &mut Tx) {
    tx.instant();
    if let Some(o) = host.connect_mut().dialog_mut() {
        o.active = None;
    }
}

fn set_active_on_hover(host: &mut dyn ConnectHost, key: &str, inside: bool, tx: &mut Tx) {
    if inside && let Some(o) = host.connect_mut().dialog_mut() {
        tx.instant();
        o.active = Some(key.to_owned());
    }
}

/// Tokenized case-insensitive search matching providers/models/search.ts (matchesModelSearch).
fn matches_model_search(query: &str, model: &Model, provider_name: &str) -> bool {
    let q = query.trim();
    if q.is_empty() {
        return true;
    }
    let hay = format!("{} {} {}", model.name, model.id, provider_name).to_lowercase();
    q.split_whitespace().all(|tok| hay.contains(&tok.to_lowercase()))
}

/// First case-insensitive match span of query in label (for SearchHighlight).
fn highlight_span(label: &str, query: &str) -> Option<std::ops::Range<usize>> {
    let q = query.trim();
    if q.is_empty() {
        return None;
    }
    let lower = label.to_lowercase();
    let needle = q.to_lowercase();
    let pos = lower.find(&needle).or_else(|| {
        let first = needle.split_whitespace().next()?;
        lower.find(first)
    })?;
    let end = pos + needle.len().min(label.len() - pos);
    (label.is_char_boundary(pos) && label.is_char_boundary(end)).then_some(pos..end)
}

/// Drops a common Console workspace prefix ("Workspace / Provider" -> "Provider") when all
/// Console groups share one, matching consoleProviderName.
fn workspace_prefix(groups: &[(ProviderInfo, Vec<Model>)]) -> Option<String> {
    if groups.len() < 2 {
        return None;
    }
    let first = groups[0].0.name.split_once(" / ")?.0;
    let prefix = format!("{first} / ");
    groups.iter().all(|(p, _)| p.name.starts_with(&prefix)).then_some(prefix)
}

fn filtered_rows(host: &dyn ConnectHost, id: &str) -> (usize, Option<String>, Vec<ModelRow>) {
    let groups = connection_groups(host, id);
    let group_count = groups.len();
    let prefix = (id == CONSOLE).then(|| workspace_prefix(&groups)).flatten();
    let query = host.connect().dialog().map_or("", |o| o.search.text.as_str());
    let mut rows = Vec::new();
    for (provider, models) in groups {
        for model in models {
            if matches_model_search(query, &model, &provider.name) {
                rows.push(ModelRow { provider: provider.clone(), model });
            }
        }
    }
    (group_count, prefix, rows)
}

/// Up / Down / Enter keyboard navigation in the connected models search list.
pub fn models_key(host: &mut dyn ConnectHost, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let Some(id) = host.connect().dialog().and_then(|o| o.dialog.provider.as_ref()).map(|p| p.id.clone()) else {
        return false;
    };
    let (_, _, rows) = filtered_rows(host, &id);
    let keys: Vec<String> = rows.into_iter().map(|r| r.model.key).collect();
    let Some(open) = host.connect_mut().dialog_mut() else { return false };
    match ev.key {
        Key::Down | Key::Up if !keys.is_empty() => {
            let at = open.active.as_ref().and_then(|a| keys.iter().position(|k| k == a));
            let n = keys.len();
            let next = match (at, ev.key == Key::Down) {
                (None, true) => 0,
                (None, false) => n - 1,
                (Some(i), true) => (i + 1) % n,
                (Some(i), false) => (i + n - 1) % n,
            };
            let picked = keys[next].clone();
            open.active = Some(picked.clone());
            tx.instant();
            tx.scroll_into_view_in("connect-models-list", format!("connected-model-{picked}"));
            true
        }
        Key::Enter => {
            let key = open
                .active
                .as_ref()
                .filter(|a| keys.iter().any(|k| k == *a))
                .cloned()
                .or_else(|| keys.first().cloned());
            let Some(key) = key else { return false };
            confirm_model(host, &key, tx);
            true
        }
        _ => false,
    }
}

/// Renders the searchable ConnectedModelPicker inside connect-dialog (no DialogHeader, matching
/// connected-models.tsx + dialog.tsx:130-145).
pub fn view<H: ConnectHost + 'static>(s: &H, open: &Open, p: &Pal, id: &str, provider_name: &str, remote: Option<El>) -> El {
    let (group_count, prefix, rows) = filtered_rows(s, id);
    let query = open.search.text.as_str();
    let active_key = open
        .active
        .as_deref()
        .filter(|a| rows.iter().any(|r| r.model.key == *a))
        .or_else(|| rows.first().map(|r| r.model.key.as_str()));

    let placeholder = opencode_i18n::t!("provider.connect.models.search", provider = provider_name);
    let list_label = opencode_i18n::t!("provider.connect.models.list", provider = provider_name);
    let field = pop::search_field(p, "connect-models-search", &open.search, &placeholder)
        .label(list_label)
        .active_descendant(active_key.map(|k| format!("connected-model-{k}")).unwrap_or_default())
        .on_edit(|s: &mut H| s.connect_mut().search_mut())
        .on_input_change(|s: &mut H, tx: &mut Tx| clear_active(s, tx))
        .on_input_key(|s: &mut H, ev, tx| crate::key(s, ev, tx));

    let trailing: Vec<El> = remote.into_iter().collect();
    let mut list = pop::List::new("connect-models-list", LIST).stuck_headers(p);
    let multi = group_count > 1;
    let mut last_group = "";
    for r in &rows {
        if multi && r.provider.id != last_group {
            let heading = match &prefix {
                Some(pre) => r.provider.name.strip_prefix(pre).unwrap_or(&r.provider.name),
                None => &r.provider.name,
            };
            list.header(p, format!("connect-model-group-{}", r.provider.id), heading);
            last_group = &r.provider.id;
        }
        let lit = active_key == Some(r.model.key.as_str());
        let name_el = match highlight_span(&r.model.name, query) {
            Some(span) => rich(r.model.name.clone(), vec![Span::new(span.start, span.end).color(p.accent)]),
            None => text(r.model.name.clone()),
        };
        let pick_key = r.model.key.clone();
        let hover_key = r.model.key.clone();
        let item = row()
            .key(format!("connected-model-{}", r.model.key))
            .role("option")
            .label(r.model.name.clone())
            .transition(Transition::none())
            .h(LIST.row_h)
            .w_full()
            .pl(10.0)
            .pr(LIST.pad_r)
            .center_y()
            .radius(LIST.row_radius)
            .fill(if lit { p.hover } else { rune::Color::TRANSPARENT })
            .on_hover_change(move |s: &mut H, inside, tx: &mut Tx| set_active_on_hover(s, &hover_key, inside, tx))
            .on_press_tx(move |s: &mut H, tx: &mut Tx| confirm_model(s, &pick_key, tx))
            .child(name_el.key("label").font(UI).color(p.text).shrink().min_w(0.0).truncate());
        list.option(item, lit);
    }
    if rows.is_empty() {
        list.other(
            el().key("connect-models-empty")
                .w_full()
                .h(EMPTY_H)
                .center()
                .transition(Transition::none())
                .child(text(opencode_i18n::t!("dialog.model.empty")).font(UI).color(p.muted)),
            EMPTY_H,
        );
    }

    let results = list.build("connect-models-scroll", f32::MAX).shrink().min_h(0.0);
    let results = if multi { results } else { results.pt(6.0) };

    col()
        .key("connect-body")
        .w_full()
        .grow(1.0)
        .shrink()
        .min_h(0.0)
        .child(pop::search_row(p, "connect-models-search-row", field, trailing))
        .child(results)
}
