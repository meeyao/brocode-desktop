//! Per-tick engine work outside `handle`: deferred isolated-context loads, inspector picks and
//! `ClearSite` cookie deletions (moved verbatim from `engine.rs` to stay under its 1,500-line cap).

use std::sync::atomic::Ordering;

use cef::{ImplBrowser, ImplBrowserHost};
use serde_json::json;

use super::EngineManager;
use crate::handlers::CdpPending;
use crate::wire::{EngineMsg, PickedElement};

impl EngineManager {
    pub fn poll_picks(&mut self) {
        for (&tab_id, t) in &self.tabs {
            crate::cdp_ledger::flush_outbox(&t.cdp_ledger, &t.host);
            crate::input_guard::pump(
                tab_id,
                &t.input_guard,
                &t.browser,
                &t.host,
                t.is_popup,
                t.isolated,
                t.preview.is_some(),
                &t.snap,
                &t.prompts,
                &self.permissions,
                &t.cdp_ledger,
                &t.emulation_scale,
                &t.cdp_id_map,
                &self.next_cdp_id,
                &self.out_tx,
            );
        }
        for outcome in self.contexts.drain_ready_loads() {
            match outcome {
                crate::context::DeferredOutcome::Load { tab_id, generation, url } => {
                    if self.tabs.get(&tab_id).is_some_and(|t| t.generation == generation)
                        && self.contexts.is_tab_ready(tab_id)
                        && !self.gate_remote_sign_in_nav(tab_id, &url)
                    {
                        self.load_released_tab_url(tab_id, generation, url);
                    }
                }
                crate::context::DeferredOutcome::Refuse { tab_id, generation, reason } => {
                    if self.tabs.get(&tab_id).is_some_and(|t| t.generation == generation) {
                        self.close_and_refuse_tab(tab_id, reason);
                    }
                }
            }
        }
        let mut ready = Vec::new();
        let mut clears = Vec::new();
        for (&tab_id, tab) in &self.tabs {
            for backend_id in tab.internal_picked.lock().unwrap().drain(..) {
                ready.push((tab_id, backend_id));
            }
            for clear in tab.pending_clears.lock().unwrap().drain(..) {
                clears.push(clear);
            }
        }
        self.poll_cookie_mirror();
        for clear in clears {
            if let Some(tab) = self.tabs.get(&clear.tab_id) {
                if !self.contexts.is_tab_ready(clear.tab_id) {
                    let reason = self.contexts.not_ready_reason(clear.tab_id);
                    let _ = self.out_tx.send(EngineMsg::CdpResponse {
                        tab_id: clear.tab_id,
                        request_id: clear.request_id,
                        ok: false,
                        result: json!({ "message": reason }),
                    });
                    continue;
                }
                for (name, domain, path) in clear.cookies {
                    self.send_internal_cdp(
                        &tab.host,
                        "Network.deleteCookies",
                        json!({ "name": name, "domain": domain, "path": path }),
                    );
                }
                self.send_internal_cdp(
                    &tab.host,
                    "Storage.clearDataForOrigin",
                    json!({
                        "origin": clear.origin,
                        "storageTypes": "local_storage,indexeddb,service_workers,cache_storage,file_systems,shader_cache",
                    }),
                );
                tab.browser.reload();
                let _ = self.out_tx.send(EngineMsg::CdpResponse {
                    tab_id: clear.tab_id,
                    request_id: clear.request_id,
                    ok: true,
                    result: json!({}),
                });
            }
        }
        for (tab_id, backend_id) in ready {
            let eref = format!("@e{}", self.next_eref);
            self.next_eref += 1;
            if let Some(tab) = self.tabs.get(&tab_id) {
                // Turn off inspect mode and highlight the chosen node cleanly.
                self.send_internal_cdp(
                    &tab.host,
                    "Overlay.setInspectMode",
                    json!({ "mode": "none", "highlightConfig": {} }),
                );
                self.send_internal_cdp(
                    &tab.host,
                    "Overlay.highlightNode",
                    json!({
                        "backendNodeId": backend_id,
                        "highlightConfig": {
                            "showInfo": true,
                            "contentColor": { "r": 56, "g": 189, "b": 248, "a": 0.28 },
                            "borderColor": { "r": 56, "g": 189, "b": 248, "a": 0.90 }
                        }
                    }),
                );
            }
            let _ = self.out_tx.send(EngineMsg::InspectState {
                tab_id,
                active: false,
                picked: Some(PickedElement {
                    tab_id,
                    eref: eref.clone(),
                    backend_node_id: backend_id,
                    selector: format!("node#{backend_id}"),
                    label: format!("{eref} (node #{backend_id})"),
                    role: None,
                    name: None,
                    text: None,
                    rect: [0.0; 4],
                }),
            });
        }
    }

    pub(super) fn handle_clear_site(&mut self, tab_id: u64, request_id: u64) {
        let Some(t) = self.tabs.get(&tab_id) else { return };
        if !self.contexts.is_tab_ready(tab_id) {
            let reason = self.contexts.not_ready_reason(tab_id);
            let _ = self.out_tx.send(EngineMsg::CdpResponse {
                tab_id,
                request_id,
                ok: false,
                result: json!({ "message": reason }),
            });
            return;
        }
        let url = t.snap.lock().unwrap().url.clone();
        let Some(origin) = browser_policy::destination_origin(&url) else {
            let _ = self.out_tx.send(EngineMsg::CdpResponse {
                tab_id,
                request_id,
                ok: true,
                result: json!({}),
            });
            return;
        };
        let msg_id = self.next_cdp_id.fetch_add(1, Ordering::Relaxed);
        t.cdp_id_map
            .lock()
            .unwrap()
            .insert(msg_id, CdpPending::ClearSite { request_id, origin });
        let raw = json!({
            "id": msg_id,
            "method": "Network.getCookies",
            "params": { "urls": [url] },
        })
        .to_string();
        if t.host.send_dev_tools_message(Some(raw.as_bytes())) != 1 {
            t.cdp_id_map.lock().unwrap().remove(&msg_id);
            let _ = self.out_tx.send(EngineMsg::CdpResponse {
                tab_id,
                request_id,
                ok: false,
                result: json!({ "message": "send_dev_tools_message rejected" }),
            });
        }
    }
}
