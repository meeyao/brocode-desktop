//! OpenCode Console-managed MCP server and skill detection and link builders
//! (`packages/app/src/settings/providers/console-extensions.ts`, `anomalyco/opencode#63125`).
//!
//! Recognizes Console's managed integration-gateway MCP entries from `/api/config` documents,
//! detects when a user's local/project config replaces a same-named Console MCP server, and builds
//! safe `http(s)` links to manage or connect Console MCPs and virtual `skill://` Console skills.

use std::collections::HashMap;

use base64::Engine as _;
use serde_json::Value;

use crate::client::encode_component;

pub const CONSOLE_URL: &str = "https://apibrother.com/console";
const GATEWAY_MARKER: &str = "/api/integration-gateway/";
const GATEWAY_SUFFIX: &str = "/mcp";
const BUILTIN_CONSOLE_SKILLS: [&str; 3] = ["opencode", "report", "browser"];

/// One MCP server delivered by OpenCode Console's managed integration gateway.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleMcp {
    pub name: String,
    pub server: String,
    pub slug: String,
    pub workspace: Option<String>,
    /// True when Console pins protocol `2026-07-28` (offers tools before the member connects).
    pub needs_connection: bool,
    /// True when the effective config replaces Console's server with another server of the same name.
    pub overridden: bool,
}

/// Console server origin/prefix and workspace slug shared by Console's gateway MCPs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleWorkspace {
    pub server: String,
    pub workspace: String,
}

/// External Console links for a Console-managed MCP server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleMcpLinks {
    pub manage: String,
    pub connect: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GatewaySource {
    server: String,
    slug: String,
    workspace: Option<String>,
    needs_connection: bool,
}

/// Recognizes Console-managed gateway MCP servers from `entries` (default config documents) and
/// marks each `overridden` if `active` (effective config documents, defaulting to `entries`) maps
/// the same MCP server name to a different gateway or non-gateway server.
pub fn console_mcps(entries: &[Value], active: Option<&[Value]>) -> Vec<ConsoleMcp> {
    let mut offered: HashMap<String, GatewaySource> = HashMap::new();
    for entry in entries {
        if entry["type"].as_str() != Some("document") {
            continue;
        }
        let Some(servers) = entry["info"]["mcp"]["servers"].as_object() else { continue };
        for (name, config) in servers {
            if let Some(source) = gateway(config) {
                offered.insert(name.clone(), source);
            }
        }
    }

    let effective_entries = active.unwrap_or(entries);
    let mut effective: HashMap<&str, &Value> = HashMap::new();
    for entry in effective_entries {
        if entry["type"].as_str() != Some("document") {
            continue;
        }
        let Some(servers) = entry["info"]["mcp"]["servers"].as_object() else { continue };
        for (name, config) in servers {
            effective.insert(name.as_str(), config);
        }
    }

    let mut items: Vec<(String, GatewaySource)> = offered.into_iter().collect();
    items.sort_by(|(a, _), (b, _)| rune::text::compare_names(a, b));
    items
        .into_iter()
        .map(|(name, source)| {
            let owner = effective.get(name.as_str()).and_then(|cfg| gateway(cfg));
            let overridden = owner
                .as_ref()
                .is_none_or(|o| o.server != source.server || o.slug != source.slug);
            ConsoleMcp {
                name,
                server: source.server,
                slug: source.slug,
                workspace: source.workspace,
                needs_connection: source.needs_connection,
                overridden,
            }
        })
        .collect()
}

/// The Console and workspace the gateway servers belong to; every server Console delivers names the
/// same workspace.
pub fn console_workspace(mcps: &[ConsoleMcp]) -> Option<ConsoleWorkspace> {
    mcps.iter().find_map(|item| {
        let workspace = item.workspace.as_deref().filter(|w| !w.is_empty())?;
        Some(ConsoleWorkspace {
            server: item.server.clone(),
            workspace: workspace.to_owned(),
        })
    })
}

/// Manage and connect URLs for a Console MCP entry.
pub fn console_mcp_links(item: &ConsoleMcp) -> ConsoleMcpLinks {
    let Some(workspace) = item.workspace.as_deref().filter(|w| !w.is_empty()) else {
        return ConsoleMcpLinks {
            manage: item.server.clone(),
            connect: item.server.clone(),
        };
    };
    let root = format!("{}/{}/extensions", item.server, encode_component(workspace));
    ConsoleMcpLinks {
        manage: format!("{root}/mcp/{}", encode_component(&item.slug)),
        connect: format!("{root}?connect={}", encode_component(&item.slug)),
    }
}

/// Where a Console skill is managed. Virtual `skill://<slug>/...` skills count as Console's while
/// Console is signed in, excluding the built-in skills OpenCode ships (`opencode`, `report`,
/// `browser`).
pub fn console_skill_link(
    id: &str,
    path: &str,
    workspace: Option<&ConsoleWorkspace>,
    signed_in: bool,
) -> Option<String> {
    if !signed_in || BUILTIN_CONSOLE_SKILLS.contains(&id) {
        return None;
    }
    let rest = path.strip_prefix("skill://")?;
    let (slug, _) = rest.split_once('/')?;
    if slug.is_empty() {
        return None;
    }
    let Some(ws) = workspace else {
        return Some(CONSOLE_URL.to_owned());
    };
    Some(format!(
        "{}/{}/extensions?kind=skills&search={}",
        ws.server,
        encode_component(&ws.workspace),
        encode_component(slug),
    ))
}

fn gateway(config: &Value) -> Option<GatewaySource> {
    if config["type"].as_str() != Some("remote") || config["oauth"].as_bool() != Some(false) {
        return None;
    }
    let raw_url = config["url"].as_str()?;
    let (server, slug) = parse_gateway_url(raw_url)?;
    Some(GatewaySource {
        server,
        slug,
        workspace: gateway_workspace(&config["headers"]),
        needs_connection: config["protocol"].as_str() == Some("2026-07-28"),
    })
}

/// Parses an `http://` or `https://` integration-gateway URL matching
/// `^(.*)/api/integration-gateway/([^/]+)/mcp$` on its pathname, returning `(server, slug)`.
fn parse_gateway_url(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    let (scheme, after_scheme) = if let Some(rest) = trimmed.strip_prefix("https://") {
        ("https", rest)
    } else {
        ("http", trimmed.strip_prefix("http://")?)
    };
    let slash = after_scheme.find('/')?;
    let authority = &after_scheme[..slash];
    if authority.is_empty() || authority.contains('?') || authority.contains('#') {
        return None;
    }
    let path_and_more = &after_scheme[slash..];
    let path_end = path_and_more.find(['?', '#']).unwrap_or(path_and_more.len());
    let pathname = &path_and_more[..path_end];

    let without_suffix = pathname.strip_suffix(GATEWAY_SUFFIX)?;
    let marker_at = without_suffix.rfind(GATEWAY_MARKER)?;
    let prefix = &without_suffix[..marker_at];
    let slug = &without_suffix[marker_at + GATEWAY_MARKER.len()..];
    if slug.is_empty() || slug.contains('/') {
        return None;
    }
    Some((format!("{scheme}://{authority}{prefix}"), slug.to_owned()))
}

/// Decodes Console's `Authorization: Bearer ocgw_<base64url claims>.<signature>` workspace (`w`).
fn gateway_workspace(headers: &Value) -> Option<String> {
    let map = headers.as_object()?;
    let auth = map
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))?
        .1
        .as_str()?;
    let after = auth.strip_prefix("Bearer ocgw_")?;
    let (claims, _) = after.split_once('.')?;
    if claims.is_empty() {
        return None;
    }
    let unpadded = claims.trim_end_matches('=');
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(unpadded).ok()?;
    let json: Value = serde_json::from_slice(&bytes).ok()?;
    let w = json["w"].as_str()?;
    (!w.is_empty()).then(|| w.to_owned())
}
