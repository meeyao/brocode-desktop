//! Tier 2 CDP Freeze and Tier 3 Hibernate/Wake operations on [`BrowserHost`].

use rune::time::Instant;

use crate::extensions::PopupState;
use crate::installer::EngineStatus;
use crate::prompts::{PermissionScope, SiteSetting};
use crate::protocol::{ContextKind, HostMsg, PermissionKind};
use crate::residency::{ExtensionWakeRequest, ResidencyEvent, step};

use super::BrowserHost;

impl BrowserHost {
    /// Whether the connected engine supports `FreezeTab`, `HibernateTab`, and `ExtensionWakeTab`.
    pub fn engine_tab_hibernation(&self) -> bool {
        self.caps.tab_hibernation()
    }

    /// Whether `tab_id` currently holds an active RPC unthrottle hold.
    pub fn is_tab_unthrottled(&self, tab_id: u64) -> bool {
        self.unthrottled_tabs.contains(&tab_id)
    }

    /// Whether `tab_id` holds an allowed `Notifications` grant (visit-scoped or persisted/session).
    pub fn tab_has_notification_grant(&self, tab_id: u64) -> bool {
        let Some(t) = self.tab(tab_id) else {
            return false;
        };
        let visit_allowed = t.prompts.visit_permissions.iter().any(|((_, kind), setting)| {
            *setting == SiteSetting::Allow && *kind == PermissionKind::Notifications
        });
        if visit_allowed {
            return true;
        }
        let Some(origin) = crate::policy::destination_origin(&t.url) else {
            return false;
        };
        let scope = PermissionScope::from_context(&t.context);
        self.site_permissions
            .get_scoped(&scope, &origin, PermissionKind::Notifications)
            == Some(SiteSetting::Allow)
    }

    /// Whether an extension popup is currently open or opening anchored to `tab_id`.
    pub fn has_extension_popup_for(&self, tab_id: u64) -> bool {
        matches!(
            self.extensions.popup,
            PopupState::Open { about_tab: Some(tid), .. }
                | PopupState::AwaitingUrl { about_tab: Some(tid), .. }
                if tid == tab_id
        )
    }

    /// Whether any extension wake requests are queued for desktop to drain.
    pub fn has_pending_ext_wakes(&self) -> bool {
        !self.pending_ext_wakes.is_empty()
    }

    /// Drains extension wake requests emitted when an extension targeted a hibernated tab.
    pub fn drain_extension_wake_tabs(&mut self) -> impl Iterator<Item = ExtensionWakeRequest> + '_ {
        self.pending_ext_wakes.drain(..)
    }

    /// Initial residency for a newly added tab: `Active` if already visible/unthrottled,
    /// otherwise `Hidden { since: now }` so background-opened tabs enter the freeze timer
    /// even when `visible_tabs` does not change.
    pub(crate) fn initial_residency(&self, tab_id: u64) -> crate::residency::TabResidency {
        if self.visible_tabs.contains(&tab_id) || self.unthrottled_tabs.contains(&tab_id) {
            crate::residency::TabResidency::Active
        } else {
            crate::residency::TabResidency::Hidden { since: Instant::now() }
        }
    }

    /// Synchronizes every tab's `TabResidency` against `visible_tabs` and `unthrottled_tabs`.
    pub(crate) fn sync_all_residency(&mut self, now: Instant) {
        let visible = &self.visible_tabs;
        let unthrottled = &self.unthrottled_tabs;
        for t in &mut self.tabs {
            let awake = visible.contains(&t.id) || unthrottled.contains(&t.id);
            let ev = if awake {
                ResidencyEvent::BecameAwake
            } else {
                ResidencyEvent::BecameHidden { now }
            };
            step(&mut t.residency, ev);
        }
    }

    /// Synchronizes one tab's `TabResidency` against `visible_tabs` and `unthrottled_tabs`.
    pub(crate) fn sync_tab_residency(&mut self, tab_id: u64, now: Instant) {
        let awake = self.visible_tabs.contains(&tab_id) || self.unthrottled_tabs.contains(&tab_id);
        if let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            let ev = if awake {
                ResidencyEvent::BecameAwake
            } else {
                ResidencyEvent::BecameHidden { now }
            };
            step(&mut t.residency, ev);
        }
    }

    /// Tier 2 CDP Freeze / Unfreeze for `tab_id`, gated on `Ready.tab_hibernation` and deduplicated
    /// via `BrowserTab::residency`. Returns `true` when a `HostMsg::FreezeTab` was sent.
    pub fn freeze_tab(&mut self, tab_id: u64, frozen: bool) -> bool {
        if !self.is_connected() || !self.caps.tab_hibernation() {
            return false;
        }
        let now = Instant::now();
        let send_freeze = if let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            if !t.has_engine_tab || !t.engine_confirmed {
                return false;
            }
            step(&mut t.residency, ResidencyEvent::SetFrozen { frozen, now }).send_freeze
        } else {
            return false;
        };
        if let Some(f) = send_freeze {
            self.send(HostMsg::FreezeTab { tab_id, frozen: f });
            true
        } else {
            false
        }
    }

    /// Tier 3 hibernation: tears down `tab_id`'s backing CEF browser to reclaim its renderer
    /// process while preserving UI metadata, context, zoom, device emulation, downloads, and prompts.
    pub fn hibernate_tab(&mut self, tab_id: u64) -> bool {
        if !self.is_connected() || !self.caps.tab_hibernation() {
            return false;
        }
        if self.visible_tabs.contains(&tab_id) || self.unthrottled_tabs.contains(&tab_id) {
            return false;
        }
        let now = Instant::now();
        let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab_id) else {
            return false;
        };
        if !t.has_engine_tab || t.loading {
            return false;
        }
        let effects = step(&mut t.residency, ResidencyEvent::Hibernate { now });
        if !effects.send_hibernate {
            return false;
        }
        t.surface.clear();
        t.has_engine_tab = false;
        t.engine_confirmed = false;
        t.loading = false;
        t.audible = false;
        t.can_go_back = false;
        t.can_go_forward = false;
        t.sent_resize = None;
        t.scroll_cursor = Default::default();
        t.find.step(crate::find::FindEvent::DocumentChanged);
        self.forget_dmabuf_pool(tab_id);
        self.pending_opens.retain(|(tid, _)| *tid != tab_id);
        self.cancel_download_save_requests(tab_id);
        self.send(HostMsg::HibernateTab { tab_id });
        self.sync_has_tabs();
        true
    }

    /// Wakes a hibernated `tab_id` at its preserved URL and context.
    pub fn wake_tab(&mut self, tab_id: u64, focused: bool) -> bool {
        let Some((url, context)) = self.tabs.iter().find(|t| t.id == tab_id).and_then(|t| {
            (t.residency.is_hibernated() && !t.has_engine_tab).then(|| (t.url.clone(), t.context.clone()))
        }) else {
            return false;
        };
        self.wake_tab_inner(tab_id, url, context, focused, false)
    }

    /// Wakes a hibernated `tab_id` directly to `raw_url` in `context`.
    /// Rejects preview tabs and enforces `policy::refusal` before waking.
    pub fn wake_tab_to_url(&mut self, tab_id: u64, raw_url: &str, context: ContextKind, focused: bool) -> bool {
        let url = crate::policy::with_scheme(raw_url);
        let connected = self.is_connected();
        let loaded_ext = self.is_loaded_extension_url(&url) && (self.engine_extensions() || !connected);
        let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab_id) else {
            return false;
        };
        if !t.residency.is_hibernated() || t.has_engine_tab || t.preview_spec.is_some() {
            return false;
        }
        let allow_ext =
            t.allow_extension_pages && !context.is_isolated() && !t.prompts.agent_driven && loaded_ext;
        if !allow_ext
            && let Some(why) = crate::policy::refusal(&url, &[])
        {
            t.loading = false;
            t.refusal = Some(why.code().to_owned());
            t.rejected = t.rejected.saturating_add(1);
            return false;
        }
        self.wake_tab_inner(tab_id, url, context, focused, true)
    }

    fn wake_tab_inner(
        &mut self,
        tab_id: u64,
        url: String,
        mut context: ContextKind,
        focused: bool,
        update_url: bool,
    ) -> bool {
        // Refresh `RemoteServer` loopback proxy from live `remote_proxies` and fail closed if absent.
        if let ContextKind::RemoteServer { server_id, proxy } = &mut context {
            let sid = server_id.trim().to_owned();
            *server_id = sid.clone();
            *proxy = self.remote_proxies.get(&sid).cloned();
            let valid = proxy.as_ref().is_some_and(|cfg| {
                crate::policy::validate_proxy_endpoint(
                    &cfg.host,
                    cfg.port,
                    cfg.username.as_deref(),
                    cfg.password.as_deref(),
                )
                .is_ok()
            });
            if !valid {
                if let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                    t.loading = false;
                    t.refusal = Some("browser.proxy.required".to_owned());
                    t.rejected = t.rejected.saturating_add(1);
                }
                return false;
            }
        }
        let ready = self.status() == EngineStatus::Ready;
        let engine_partitions = self.engine_partitions();
        let engine_remote_proxy = self.engine_remote_proxy();
        let engine_preview = self.engine_preview();
        let has_preview = self
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .is_some_and(|t| t.preview_spec.is_some());
        if ready && !context.is_supported(engine_partitions, engine_remote_proxy) {
            if let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                t.loading = false;
                t.refusal = Some("browser.partitions.unsupported".to_owned());
                t.rejected = t.rejected.saturating_add(1);
            }
            return false;
        }
        if has_preview && ready && !engine_preview {
            if let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                t.loading = false;
                t.refusal = Some("browser.preview.unsupported".to_owned());
                t.rejected = t.rejected.saturating_add(1);
            }
            return false;
        }

        let had_engine_tabs = self.has_engine_tabs();
        let connected = self.is_connected();
        let is_ext_url = crate::policy::extension_origin(&url);
        let can_send_now = connected
            && ((context.is_default()
                && !has_preview
                && !is_ext_url
                && !crate::policy::is_file_scheme(&url))
                || ready);

        let Some((
            (width_pt, height_pt),
            scale,
            preview_spec,
            agent_driven,
            allow_ext_pages,
            zoom,
            emulate,
        )) = self.tabs.iter_mut().find(|t| t.id == tab_id).map(|t| {
            if update_url {
                t.url = url.clone();
            }
            t.context = context.clone();
            t.refusal = None;
            t.load_error = None;
            t.loading = true;
            t.has_engine_tab = true;
            t.engine_confirmed = false;
            step(&mut t.residency, ResidencyEvent::BeginWake);
            let emulate = t
                .device
                .as_ref()
                .map(|d| (d.clone(), d.fit([t.viewport_pt.0, t.viewport_pt.1])));
            if can_send_now {
                t.sent_resize = Some((t.viewport_pt.0, t.viewport_pt.1, t.scale));
            }
            (
                t.viewport_pt,
                t.scale,
                t.preview_spec.clone(),
                t.prompts.agent_driven,
                t.allow_extension_pages,
                t.zoom,
                emulate,
            )
        }) else {
            return false;
        };

        if focused {
            self.focused_tab = Some(tab_id);
        }
        if !had_engine_tabs {
            self.clear_stale_fault();
        }
        self.sync_has_tabs();

        if can_send_now {
            let open_url = if preview_spec.is_some() {
                "about:blank".to_owned()
            } else {
                url
            };
            self.send_open_tab_with_ext(
                tab_id,
                open_url,
                width_pt,
                height_pt,
                scale,
                focused,
                Vec::new(),
                context,
                preview_spec,
                agent_driven,
                allow_ext_pages,
                None,
            );
            if (zoom - 1.0).abs() > 0.001 {
                self.send(HostMsg::SetZoom { tab_id, factor: zoom });
            }
            if let Some((device, emu_scale)) = emulate {
                self.send(HostMsg::Emulate {
                    tab_id,
                    device: Some(device),
                    scale: emu_scale,
                });
            }
            self.replay_tab_lifecycle(tab_id);
        } else {
            self.pending_opens.retain(|(id, _)| *id != tab_id);
            self.pending_opens.push((tab_id, url));
            self.ensure_connected();
        }
        true
    }
}
