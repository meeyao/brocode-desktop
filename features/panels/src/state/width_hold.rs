//! Chrome-style tab-width hold while closing side panel tabs with the mouse
//! (`packages/app/src/runtime/plugin/tab-strip-widths.ts`, `opencode#66887`):
//!
//! Closing a non-last tab with the mouse holds end padding equal to the closed tab's width plus
//! the strip gap so the next tab's close button slides directly under the pointer. Closing the
//! last tab keeps the existing hold so the new last close button lands under the pointer.
//! Leaving the strip, blurring the window, resizing the strip, opening/moving a tab, or closing
//! via keyboard releases the hold so tabs expand to fill the strip again.

use std::cell::{Cell, RefCell};

use crate::SessionPane;
use crate::host::PanelsHost;

/// A tab strip's scroll, as `tab-strip-scroll.ts` keeps it: the offset a wheel notch moves on
/// from, and the session, width and lead tab it last laid out with, so a strip that grows glides
/// to its new tab.
#[derive(Default)]
pub struct StripScroll {
    pub x: Cell<f32>,
    pub laid: RefCell<Option<(String, f32, bool)>>,
}

/// Explicit state machine for one pane's tab-strip width hold (`tab-strip-widths.ts`).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum StripWidthHold {
    #[default]
    Released,
    Holding {
        /// End-padding hold in points subtracted from the strip's available room.
        hold: f32,
        /// Pane width when held; resizing releases the hold.
        pane_w: f32,
        /// Expected tab count after the close; any other count change releases the hold.
        count: usize,
        /// Session ID for which this hold applies.
        session_id: String,
    },
}

/// Inputs describing a mouse-initiated tab close on a strip.
#[derive(Clone, Debug)]
pub struct MouseClose {
    pub session_id: String,
    pub pane_w: f32,
    pub closed_w: f32,
    pub is_last: bool,
    pub overflowing: bool,
    pub remaining: usize,
    pub gap: f32,
}

pub enum WidthHoldEvent {
    MouseClose(MouseClose),
    Release,
}

impl StripWidthHold {
    /// Transitions the hold state.
    pub fn step(&mut self, event: WidthHoldEvent) {
        match event {
            WidthHoldEvent::Release => *self = Self::Released,
            WidthHoldEvent::MouseClose(m) => {
                if m.overflowing || m.remaining == 0 {
                    *self = Self::Released;
                    return;
                }
                let prev_hold = match self {
                    Self::Holding { hold, pane_w, count, session_id }
                        if *session_id == m.session_id
                            && (*pane_w - m.pane_w).abs() < 0.5
                            && *count == m.remaining + 1 =>
                    {
                        *hold
                    }
                    _ => 0.0,
                };
                // Closing the last tab keeps the existing hold so tabs before it fill the freed
                // space and the new last close button lands under the pointer (`tab-strip-widths.ts:46-49`).
                let freed = if m.is_last { 0.0 } else { m.closed_w + m.gap };
                let hold = (prev_hold + freed).floor().max(0.0);
                *self = if hold > 0.0 {
                    Self::Holding { hold, pane_w: m.pane_w, count: m.remaining, session_id: m.session_id }
                } else {
                    Self::Released
                };
            }
        }
    }

    /// Active hold in points for the current strip layout, or `0.0` if released or invalidated
    /// by a resize, session switch, or non-mouse tab count change.
    pub fn active(&self, session_id: &str, pane_w: f32, count: usize) -> f32 {
        match self {
            Self::Holding { hold, pane_w: w, count: c, session_id: sid }
                if sid == session_id && (*w - pane_w).abs() < 0.5 && *c == count =>
            {
                *hold
            }
            _ => 0.0,
        }
    }
}

/// Releases held tab widths across all three panes (e.g. on window blur, `tab-strip-widths.ts:76`).
pub fn release_strip_width_holds(host: &mut dyn PanelsHost) {
    let holds = &mut host.side_mut().width_holds;
    if holds.iter().any(|h| !matches!(h, StripWidthHold::Released)) {
        for h in holds.iter_mut() {
            h.step(WidthHoldEvent::Release);
        }
    }
}

/// Releases held tab widths for one pane if currently held. Returns true if a hold was released.
pub fn release_pane_width_hold(host: &mut dyn PanelsHost, pane: SessionPane) -> bool {
    let slot = pane as usize;
    if !matches!(host.side().width_holds[slot], StripWidthHold::Released) {
        host.side_mut().width_holds[slot].step(WidthHoldEvent::Release);
        true
    } else {
        false
    }
}
