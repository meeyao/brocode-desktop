//! Tooltip typography and colored tooltip text (`packages/ui/src/overlays/tooltip/tooltip.css`,
//! `packages/ui/src/overlays/tooltip/tooltip.tsx`, `opencode#66789`):
//! - Compact tooltips use 4 pt vertical / 6 pt horizontal padding and `--line-height-compact`
//!   (16 pt) line-height on 11 pt medium tabular text.
//! - `TooltipText` offers five semantic color tones (`base`, `muted`, `faint`, `accent`,
//!   `warning`) for multi-tone tooltip copy.

use rune::{Color, El, TextStyle, text};

use crate::palette::Pal;

/// Compact tooltip padding: `padding: 4px 6px` (`tooltip.css:15`, `opencode#66789`).
pub const COMPACT_PADDING: (f32, f32) = (6.0, 4.0);

/// Compact tooltip line-height: `--line-height-compact` = 16 pt (`tooltip.css:25`, `opencode#66789`).
pub const COMPACT_LINE_HEIGHT: f32 = 16.0;

/// Color tone for `TooltipText` (`packages/ui/src/overlays/tooltip/tooltip.tsx:43-45`,
/// `tooltip.css:47-65`, `opencode#66789`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TooltipTextVariant {
    #[default]
    Base,
    Muted,
    Faint,
    Accent,
    Warning,
}

impl TooltipTextVariant {
    /// Foreground color from `p` matching `[data-component="tooltip-v2-text"]` (`tooltip.css:47-65`).
    pub fn color(self, p: &Pal) -> Color {
        match self {
            TooltipTextVariant::Base => p.text,
            TooltipTextVariant::Muted => p.muted,
            TooltipTextVariant::Faint => p.faint,
            TooltipTextVariant::Accent => p.accent,
            TooltipTextVariant::Warning => p.warning,
        }
    }
}

/// Compact tooltip text style (`tooltip.css:21-27`, `opencode#66789`): 11 pt weight 530,
/// `--line-height-compact` (16 pt), 0.05 tracking, tabular numerals.
pub fn compact_text_style(base: TextStyle) -> TextStyle {
    base.lh(COMPACT_LINE_HEIGHT)
}

/// Renders one `TooltipText` span in `variant`'s color (`tooltip.tsx:96-100`, `opencode#66789`).
pub fn tooltip_text(p: &Pal, variant: TooltipTextVariant, label: impl Into<String>) -> El {
    let style = TextStyle::new(11.0, 530.0)
        .lh(COMPACT_LINE_HEIGHT)
        .tracking(0.05)
        .features("'tnum' 1");
    text(label).font(style).color(variant.color(p))
}
