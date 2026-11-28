//! Every server the app knows, as Desktop's registry lists them (`runtime/server/registry.tsx`):
//! the app's own (`"sidecar"`), the servers added by address (`servers.json`), then the plugins'
//! listed servers, ready or not (a WSL distro starting, an SSH host waiting for its sign-in), so
//! Home and the health checks see them all. The runtime connects to the ones with an endpoint
//! (`servers::runtime`); what lists call each is [`label`].

use opencode_kit::servers::{HttpBase, ServerKey};
use opencode_servers::ServerConn;
use opencode_servers::plugin_server::{PluginStage, PluginStatus};
use opencode_servers::registry::{server_hint, server_label};
use opencode_plugin_spec::ServerState;

use crate::App;

/// A listed server and the name its source gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub conn: ServerConn,
    pub name: String,
}

impl Entry {
    pub fn key(&self) -> ServerKey {
        self.conn.key()
    }
}

/// The registry in Desktop's order (`resolveServerList`): the app's own server first, then the
/// servers added by address, then the plugins'. A run without a server (a fixture, a story) lists
/// only its own.
pub fn list(s: &App) -> Vec<Entry> {
    let own = crate::servers::runtime::own_server();
    let own_base = own.as_ref().map(|o| o.base.clone()).unwrap_or_else(|| s.server_base().to_owned());
    let mut own_http = HttpBase::new(&own_base);
    if let Some(auth) = own.as_ref().and_then(|o| o.auth_header()) {
        own_http = own_http.with_password(auth);
    }
    let mut out = vec![Entry { conn: ServerConn::Sidecar { http: own_http, display_name: None, label: None }, name: String::new() }];
    if own.is_none() {
        return out;
    }
    for x in crate::servers::list() {
        let Some(ServerConn::Http { http, auth_token, label, .. }) = ServerConn::http_server(&x.url) else { continue };
        let conn = ServerConn::Http { http: HttpBase { url: http.url, password: x.password.clone() }, auth_token, display_name: x.name.clone(), label };
        out.push(Entry { conn, name: x.label() });
    }
    for c in crate::plugin_host::contributed_servers(s).into_iter().filter(|c| !c.by_address() && c.rec.listed) {
        let status = match c.rec.state {
            ServerState::Ready if !c.rec.url.is_empty() => {
                let mut http = HttpBase::new(&c.rec.url);
                if !c.rec.password.is_empty() {
                    http = http.with_password(&c.rec.password);
                }
                PluginStatus::Ready { http }
            }
            ServerState::Starting => PluginStatus::Stage(PluginStage::Starting),
            ServerState::Auth => PluginStatus::Stage(PluginStage::Auth),
            ServerState::Incompatible => PluginStatus::Stage(PluginStage::Incompatible),
            ServerState::Failed | ServerState::Ready => PluginStatus::Stage(PluginStage::Failed),
            ServerState::Stopped => PluginStatus::Stage(PluginStage::Stopped),
        };
        let label = (!c.rec.label.is_empty()).then(|| c.rec.label.clone());
        let plugin = c.key.split(':').next().unwrap_or_default().to_owned();
        let conn = ServerConn::Plugin { key: ServerKey::new(&c.key), plugin, status, managed: c.rec.managed, display_name: Some(c.rec.name.clone()), label };
        out.push(Entry { conn, name: c.rec.name.clone() });
    }
    out
}

/// The servers Home lists: those not hidden from it, and when every one is, the app's own anyway
/// (`resolveVisibleServers`).
pub fn visible(s: &App) -> Vec<Entry> {
    let all = list(s);
    let hidden = crate::projects::home::hidden();
    let shown: Vec<Entry> = all.iter().filter(|e| !hidden.contains(&e.key())).cloned().collect();
    if !shown.is_empty() || all.is_empty() {
        return shown;
    }
    all.into_iter().take(1).collect()
}

/// What lists call `conn` (`servers.label`): its own name; the app's own server by the hostname it
/// last reported, else "This computer"; another by its address.
pub fn label(conn: &ServerConn) -> String {
    let hostname = conn.builtin().then(|| crate::servers::health::hostname(&conn.key())).flatten();
    server_label(conn, conn.builtin(), hostname.as_deref(), opencode_i18n::t!("server.thisComputer"))
}

/// "This computer" beside the app's own server, unless its name says it (`servers.hint`).
pub fn hint(conn: &ServerConn) -> Option<String> {
    server_hint(conn.builtin(), &label(conn), opencode_i18n::t!("server.thisComputer"))
}

/// `label` for the server keyed `key`.
pub fn label_of(s: &App, key: &ServerKey) -> String {
    list(s).into_iter().find(|e| e.key() == *key).map_or_else(|| crate::servers::runtime::label(key), |e| label(&e.conn))
}

/// `servers.tag(conn)` (`registry.tsx:346-348`, `tab-nav.tsx:202-207`): the server's label when
/// two or more servers are in the registry (ready or not) and `key` isn't the built-in server.
pub fn tag_of(s: &App, key: &ServerKey) -> Option<String> {
    let all = list(s);
    if all.len() < 2 {
        return None;
    }
    all.into_iter().find(|e| e.key() == *key).filter(|e| !e.conn.builtin()).map(|e| label(&e.conn))
}

/// The connection to `conn`'s server the probes and the folder picker use: the app's own as
/// launch found it, another with its password.
pub fn server_for(conn: &ServerConn) -> Option<opencode_kit::client::Server> {
    if conn.builtin() {
        return crate::servers::runtime::own_server();
    }
    let http = conn.http()?;
    let server = opencode_kit::client::Server::anonymous(&http.url);
    Some(match http.password.as_deref().filter(|p| !p.is_empty()) {
        Some(p) => server.with_password(p),
        None => server,
    })
}
