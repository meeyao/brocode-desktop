//! Per-tab lifecycle helpers for `ExtensionHost` (`Live`, `Frozen`, `Discarded`, `audible`).

use serde_json::json;

use super::ExtensionHost;

/// Explicit lifecycle phase for an extension-visible tab (zero illegal combinations).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ExtTabLifecycle {
    #[default]
    Live,
    Frozen,
    Discarded,
}

impl ExtTabLifecycle {
    pub(super) fn is_discarded(self) -> bool {
        self == Self::Discarded
    }

    pub(super) fn is_frozen(self) -> bool {
        self == Self::Frozen
    }
}

impl ExtensionHost {
    pub fn is_discarded_tab(&self, engine_tab_id: u64) -> bool {
        self.tabs
            .iter()
            .any(|t| t.engine_tab_id == engine_tab_id && t.lifecycle.is_discarded() && !t.agent_driven)
    }

    pub fn set_tab_frozen(&mut self, engine_tab_id: u64, frozen: bool) {
        let Some(tab) = self.tabs.iter_mut().find(|t| t.engine_tab_id == engine_tab_id) else {
            return;
        };
        if tab.lifecycle.is_discarded() || tab.lifecycle.is_frozen() == frozen {
            return;
        }
        tab.lifecycle = if frozen { ExtTabLifecycle::Frozen } else { ExtTabLifecycle::Live };
        if !tab.agent_driven {
            let tab_clone = tab.clone();
            self.emit_tab_updated(&tab_clone, json!({ "frozen": frozen }), false);
        }
    }

    pub fn on_tab_hibernated(&mut self, engine_tab_id: u64) {
        self.web_nav.remove_tab(engine_tab_id);
        let Some(tab) = self.tabs.iter_mut().find(|t| t.engine_tab_id == engine_tab_id) else {
            return;
        };
        tab.lifecycle = ExtTabLifecycle::Discarded;
        tab.loading = false;
        tab.active = false;
        tab.audible = false;
        // Keep `ext_tab_ids` intact while Discarded so extensions see a stable tab ID across
        // `tabs.onUpdated`, `tabs.{get,update,remove}`, and `tabs.onRemoved`; cleared on wake.
        if !tab.agent_driven {
            let tab_clone = tab.clone();
            self.emit_tab_updated(&tab_clone, json!({ "discarded": true }), false);
        }
    }

    pub fn on_tab_audible_changed(&mut self, engine_tab_id: u64, audible: bool) {
        let Some(tab) = self.tabs.iter_mut().find(|t| t.engine_tab_id == engine_tab_id) else {
            return;
        };
        if tab.audible == audible {
            return;
        }
        tab.audible = audible;
        if !tab.agent_driven {
            let tab_clone = tab.clone();
            self.emit_tab_updated(&tab_clone, json!({ "audible": audible }), false);
        }
    }
}
