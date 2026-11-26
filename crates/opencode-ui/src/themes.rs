//! Desktop's colour themes (Settings › Appearance › Theme). `assets/themes.json` holds each
//! of Desktop's theme files resolved as Desktop resolves them (`tools/themes.ts`), so a theme
//! here paints what it paints there. The default, OpenCode, keeps `assets/theme.json` and
//! takes only its code colours from there.
//!
//! The chosen theme is per thread, like Rune's blend mode: the view reads it through
//! `palette::pal`, and tests on other threads keep the default.

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{OnceLock, RwLock};

use serde_json::{Map, Value};

const THEMES: &str = include_str!("../../../apps/desktop/assets/themes.json");

/// Desktop's id for its default theme.
pub const DEFAULT: &str = "oc-2";

pub struct Info {
    pub id: String,
    pub name: String,
    tokens: Tokens,
}

/// A theme's tokens for light and dark, laid over `assets/theme.json`'s.
enum Tokens {
    /// One of Desktop's: its object in `themes.json`, read when the theme is first painted or
    /// compared. Reading every theme at start kept 1 MB that only the theme in use needs.
    Asset { json: &'static str, read: OnceLock<[Map<String, Value>; 2]> },
    /// A registered theme's own.
    Given([Map<String, Value>; 2]),
}

impl Info {
    fn tokens(&self) -> &[Map<String, Value>; 2] {
        match &self.tokens {
            Tokens::Given(tokens) => tokens,
            Tokens::Asset { json, read } => read.get_or_init(|| {
                let t: Value = serde_json::from_str(json).unwrap_or_else(|e| panic!("themes.json: {}: {e}", self.id));
                ["light", "dark"].map(|m| t[m].as_object().cloned().unwrap_or_default())
            }),
        }
    }
}

/// A theme's id and name in `themes.json`; its modes are skipped, not built.
#[derive(serde::Deserialize)]
struct Head {
    id: String,
    name: String,
}

fn desktop() -> &'static [Info] {
    static ALL: OnceLock<Vec<Info>> = OnceLock::new();
    ALL.get_or_init(|| {
        // Each element's own text, so one theme can be read alone later.
        let mut rest = THEMES.trim_start().strip_prefix('[').unwrap_or_else(|| panic!("themes.json: not a list"));
        let mut list: Vec<(Head, &'static str)> = Vec::new();
        loop {
            rest = rest.trim_start().trim_start_matches(',').trim_start();
            if rest.is_empty() || rest.starts_with(']') {
                break;
            }
            let mut one = serde_json::Deserializer::from_str(rest).into_iter::<Head>();
            let head = match one.next() {
                Some(Ok(head)) => head,
                Some(Err(e)) => panic!("themes.json: {e}"),
                None => break,
            };
            let (json, after) = rest.split_at(one.byte_offset());
            list.push((head, json));
            rest = after;
        }
        let asset = |json| Tokens::Asset { json, read: OnceLock::new() };
        let default = list.iter().find(|(h, _)| h.id == DEFAULT).map_or_else(|| Tokens::Given(Default::default()), |(_, json)| asset(*json));
        std::iter::once(Info { id: DEFAULT.into(), name: "Brocode".into(), tokens: default })
            .chain(list.into_iter().filter(|(h, _)| h.id != DEFAULT).map(|(h, json)| Info { id: h.id, name: h.name, tokens: asset(json) }))
            .collect()
    })
}
/// The list, Desktop's themes then the ones the host registered. Each list is built once per
/// registration and kept, so a `&'static` slice stays valid.
static LIST: RwLock<Option<&'static [&'static Info]>> = RwLock::new(None);

/// Every theme in Desktop's picker order (OpenCode, then the rest by id), then the host's own
/// (a plugin's, registered at run time).
pub fn all() -> &'static [&'static Info] {
    if let Some(list) = *LIST.read().unwrap_or_else(|e| e.into_inner()) {
        return list;
    }
    let mut list = LIST.write().unwrap_or_else(|e| e.into_inner());
    list.get_or_insert_with(|| Box::leak(desktop().iter().collect::<Vec<_>>().into_boxed_slice()))
}

/// Adds theme `id` (or replaces the one with that id, keeping its place, so its index stays
/// put) with `light` and `dark` tokens laid over OpenCode's own, so a theme may set only the
/// tokens it changes. A change moves [`generation`], so the colours built for the old tokens
/// go and a hot-reloaded theme repaints at once.
pub fn register(id: &str, name: &str, tokens: [Map<String, Value>; 2]) {
    let [light, dark] = tokens;
    // Only colours: a token that doesn't read as one keeps OpenCode's, so a theme can't break
    // the palette.
    let over = |m: usize, own: Map<String, Value>| {
        let mut full = desktop()[0].tokens()[m].clone();
        full.extend(own.into_iter().filter(|(_, v)| v.as_str().and_then(rune::color::hex).is_some()));
        full
    };
    let tokens = [over(0, light), over(1, dark)];
    let mut list: Vec<&'static Info> = all().to_vec();
    let at = list.iter().position(|t| t.id == id);
    if at.is_some_and(|i| list[i].name == name && *list[i].tokens() == tokens) {
        return;
    }
    let info: &'static Info = Box::leak(Box::new(Info { id: id.to_owned(), name: name.to_owned(), tokens: Tokens::Given(tokens) }));
    match at {
        Some(i) => list[i] = info,
        None => list.push(info),
    }
    *LIST.write().unwrap_or_else(|e| e.into_inner()) = Some(Box::leak(list.into_boxed_slice()));
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Which registration of the themes is current: it moves each time [`register`] adds or
/// changes one. Caches of a theme's colours key on it with the theme's index.
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

/// The index of `id` in [`all`], or the default's (0) for an unknown id.
pub fn index(id: &str) -> usize {
    all().iter().position(|t| t.id == id).unwrap_or(0)
}

thread_local! {
    static ACTIVE: Cell<usize> = const { Cell::new(0) };
}

/// The theme `palette::pal` paints with on this thread, as an index into [`all`].
pub fn active() -> usize {
    ACTIVE.with(Cell::get)
}

pub fn set_active(id: &str) {
    ACTIVE.with(|a| a.set(index(id)));
}

thread_local! {
    static RAISED: Cell<bool> = const { Cell::new(false) };
}

/// Whether this thread's palette draws the contrast highlights over its theme's colours:
/// the system asks for more contrast (macOS's Increase Contrast, `Cx::system_increased_contrast`).
pub fn raised_contrast() -> bool {
    RAISED.with(Cell::get)
}

pub fn set_raised_contrast(on: bool) {
    RAISED.with(|r| r.set(on));
}

/// `base` (the default theme's JSON) with theme `i`'s tokens laid over both modes (the default's
/// are its code colours alone), or None when `base` has no modes to lay them over.
pub fn json(i: usize, base: &str) -> Option<String> {
    let t = all().get(i)?;
    let mut v: Value = serde_json::from_str(base).ok()?;
    v["name"] = Value::String(t.name.clone());
    for (m, tokens) in ["light", "dark"].into_iter().zip(t.tokens()) {
        let map = v[m].as_object_mut()?;
        map.extend(tokens.iter().map(|(k, val)| (k.clone(), val.clone())));
    }
    Some(v.to_string())
}


/// The id [`contrast`] registers.
pub const CONTRAST: &str = "rune-system-contrast";

/// A theme painted only in the system contrast theme's colours (Windows' Contrast themes, CSS
/// `forced-colors: active`), registered or brought up to date for `c`; its id. Every colour
/// OpenCode's theme names becomes one of them, by what it paints: grounds, layers and their
/// washes the window colour; text, icons, borders and code the text colour (borders solid);
/// accents, selections and state colours the highlight; what sits on an accent the highlight's
/// text. The same colours set on a theme again register nothing.
pub fn contrast(c: &rune::SystemContrast) -> &'static str {
    static LAST: std::sync::Mutex<Option<rune::SystemContrast>> = std::sync::Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if last.as_ref() == Some(c) {
        return CONTRAST;
    }
    // OpenCode's theme as it paints: `theme.json` with Desktop's own tokens over it (its code
    // colours among them).
    let src = json(0, crate::palette::THEME).unwrap_or_else(|| crate::palette::THEME.to_owned());
    let base: Value = serde_json::from_str(&src).unwrap_or_default();
    let mut names: Vec<String> = ["light", "dark", "palette"].iter().filter_map(|k| base[*k].as_object()).flat_map(|m| m.keys().cloned()).collect();
    names.sort();
    names.dedup();
    let mode = |dark: bool| -> Map<String, Value> {
        let Ok(t) = rune::Theme::from_json(&src, dark) else { return Map::new() };
        let (bg, fg) = (t.try_color("background-bg-base").unwrap_or(c.window), t.try_color("text-text-base").unwrap_or(c.text));
        names.iter().filter_map(|n| t.try_color(n).map(|col| (n.clone(), Value::String(forced(n, col, bg, fg, c).hex())))).collect()
    };
    register(CONTRAST, "Contrast", [mode(false), mode(true)]);
    *last = Some(*c);
    CONTRAST
}

/// What `col`, token `name` of OpenCode's theme over ground `bg` with text `fg`, paints as in
/// contrast theme `c`.
fn forced(name: &str, col: rune::Color, bg: rune::Color, fg: rune::Color, c: &rune::SystemContrast) -> rune::Color {
    let ink = name.starts_with("text") || name.starts_with("icon") || name.starts_with("syntax") || name.starts_with("border");
    let (r, g, b) = (rune::color::srgb_encode(col.r), rune::color::srgb_encode(col.g), rune::color::srgb_encode(col.b));
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let saturated = max > 0.2 && (max - min) / max > 0.35;
    let pick = match () {
        _ if ink && name.contains("contrast") => c.highlight_text,
        _ if ink => c.text,
        _ if name.contains("contrast") || saturated => c.highlight,
        _ if (col.luminance() - bg.luminance()).abs() <= (col.luminance() - fg.luminance()).abs() => c.window,
        _ => c.text,
    };
    // Borders draw solid: a contrast theme's lines are what separates its surfaces.
    rune::Color { a: if name.starts_with("border") { 1.0 } else { col.a }, ..pick }
}