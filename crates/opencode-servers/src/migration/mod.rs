//! One-time, non-destructive migration of the per-server files Brocode kept while a
//! window held one server at a time, to the mixed-server shape a window keeps once every server
//! is live at once (Desktop's, `shell/tabs/schema.ts`, `runtime/server/persistence.ts`):
//!
//! - [`window`]: a window file (`opencode.window.<id>.dat`, `{ "<server url>": { tabs, route,
//!   drafts, side, closedTabs } }`), and for the profile's first window the `tabs.json` lists and
//!   `ui.json` pages it fell back to, become one [`super::tabs::WindowDoc`] whose tabs each carry
//!   their [`super::key::ServerKey`].
//! - [`global`]: what `ui.json` and `tabs.json` kept per server for every window (the sidebar,
//!   inbox and organized-sidebar state, the zoom, the projects used) and `servers.json`'s list
//!   become the `opencode.global.dat` keys Desktop keeps them under.
//!
//! Both are pure: they read and return JSON, and never touch a file. Both are lossless: what the
//! old shape held that the new one has no place for (another server's page, a second address of
//! the same server) stays under `legacy`, and [`window::restore`] / [`global::restore`] give back
//! exactly what the old app read. Both are idempotent: a file already in the new shape comes back
//! unchanged.

pub mod global;
pub mod window;

use opencode_kit::servers::key::{ServerKey, normalize_server_url};
use opencode_kit::servers::loopback_origin;

/// What maps an old file's server addresses to keys.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Context {
    /// The app's own local service's addresses (the default `http://127.0.0.1:4096`, the port
    /// `service.json` registered): any of them, or a loopback alias of one, is `"sidecar"`.
    pub sidecar: Vec<String>,
    /// Contributed servers' last known endpoints, each with its key (`wsl:Ubuntu`).
    pub aliases: Vec<(String, ServerKey)>,
    /// The servers saved by address (`servers.json`), in order.
    pub saved: Vec<String>,
    /// The address the window was last connected to; its page becomes the window's page.
    pub active: Option<String>,
}

impl Context {
    /// The key an old file's address `url` maps to: the sidecar's, a contributed server's, or the
    /// normalized address itself.
    pub fn key(&self, url: &str) -> ServerKey {
        let norm = normalize_server_url(url).unwrap_or_else(|| url.to_owned());
        let same = |other: &str| {
            let other = normalize_server_url(other).unwrap_or_else(|| other.to_owned());
            other == norm || loopback_origin(&other).is_some_and(|o| Some(o) == loopback_origin(&norm))
        };
        if self.sidecar.iter().any(|s| same(s)) {
            return ServerKey::sidecar();
        }
        if let Some((_, key)) = self.aliases.iter().find(|(u, _)| same(u)) {
            return key.clone();
        }
        ServerKey::new(norm)
    }

    /// Where `url` sorts: the sidecar first, then the saved servers in their order, then the rest
    /// as the file lists them.
    fn rank(&self, url: &str) -> (u8, usize) {
        let key = self.key(url);
        if key.is_sidecar() {
            return (0, 0);
        }
        match self.saved.iter().position(|s| normalize_server_url(s).as_deref() == Some(key.as_str())) {
            Some(at) => (1, at),
            None => (2, 0),
        }
    }

    /// Orders `urls` (stable within a rank, so file order breaks ties).
    fn order(&self, mut urls: Vec<String>) -> Vec<String> {
        urls.sort_by_key(|u| self.rank(u));
        urls
    }

    /// The address among `urls` whose page the window shows: the active one, else one with the
    /// active one's key, else the first.
    fn active_of<'a>(&self, urls: &'a [String]) -> Option<&'a String> {
        let active = self.active.as_deref().and_then(normalize_server_url);
        let by_url = active.as_ref().and_then(|a| urls.iter().find(|u| normalize_server_url(u).as_ref() == Some(a)));
        let by_key = active.as_ref().and_then(|a| urls.iter().find(|u| self.key(u) == self.key(a)));
        by_url.or(by_key).or(urls.first())
    }
}

/// An old file's top-level key that names a server.
pub fn is_server_url(key: &str) -> bool {
    key.starts_with("http://") || key.starts_with("https://")
}
