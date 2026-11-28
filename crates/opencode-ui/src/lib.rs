//! Shared OpenCode UI design-system foundation: palettes and themes (`palette`, `themes`),
//! typography and layout style tokens (`style`), dialog sizing (`dialog`), the keyboard focus
//! ring (`focus`), scroll containers (`scroller`), rolling odometers (`odometer`), bottom sheets (`sheet`), popovers (`popover`),
//! provider marks (`provider_mark`), shortcut keymap (`keymap`), reusable controls (`widgets`),
//! file/review panel chrome (`panel_chrome`), popup/context menus (`popup`), and toast
//! notifications (`toast`) for Rune apps (`apps/desktop`, `apps/web`, `features/*`, and other
//! embedder apps).

#![forbid(unsafe_code)]

pub mod dialog;
pub mod focus;
pub mod keymap;
pub mod odometer;
pub mod palette;
pub mod panel_chrome;
pub mod popover;
pub mod popup;
pub mod prompt_history;
pub mod provider_mark;
pub mod scroller;
pub mod sheet;
pub mod style;
pub mod tab;
pub mod themes;
pub mod toast;
pub mod tooltip;
pub mod video_overlay;
pub mod widgets;
