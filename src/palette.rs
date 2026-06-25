//! Color palette extracted verbatim from the approved Time Tracker mockup
//! (claude.ai/design). Pure data (u32 `0xRRGGBB`) — UI code wraps these with
//! `gpui::rgb(...)`. Kept gpui-free so models/tests can share it.

// --- chrome -----------------------------------------------------------------
pub const BG: u32 = 0xffffff;
pub const SURFACE: u32 = 0xfbfbfc; // sidebar / top bar / subtle panels
pub const CARD: u32 = 0xffffff;
pub const TEXT: u32 = 0x18181b; // primary
pub const TEXT_2: u32 = 0x71717a; // secondary
pub const TEXT_3: u32 = 0x8a8a93; // meta
pub const LABEL: u32 = 0x52525b; // section labels
pub const MUTED: u32 = 0xa1a1aa;
pub const FAINT: u32 = 0xb4b4bb;
pub const BORDER: u32 = 0xececef;
pub const BORDER_2: u32 = 0xf0f0f2;
pub const HAIRLINE: u32 = 0xf3f3f5;
pub const HAIRLINE_2: u32 = 0xf5f5f7;
pub const HOVER: u32 = 0xf7f7f9;

// --- accents ----------------------------------------------------------------
pub const ACCENT: u32 = 0x4f46e5; // indigo-600
pub const ACCENT_DK: u32 = 0x4338ca;
pub const ACCENT_SOFT: u32 = 0xeef0fe;
pub const DANGER: u32 = 0xe11d48; // stop
pub const RUNNING: u32 = 0x22c55e; // running dot
pub const DONE: u32 = 0x16a34a; // export done

// --- per-project palette (main / soft bg / dark text) -----------------------
pub struct Pal {
    pub main: u32,
    pub soft: u32,
    pub text: u32,
}

pub const PROJECT_PALETTE: &[Pal] = &[
    Pal { main: 0x4f46e5, soft: 0xeef0fe, text: 0x4338ca }, // indigo
    Pal { main: 0x0d9488, soft: 0xe4f4f1, text: 0x0f766e }, // teal
    Pal { main: 0xd97706, soft: 0xfcf2e3, text: 0xb45309 }, // amber
    Pal { main: 0xe11d48, soft: 0xfde9ee, text: 0xbe123c }, // rose
    Pal { main: 0x7c3aed, soft: 0xf1ebfe, text: 0x6d28d9 }, // violet
    Pal { main: 0x0284c7, soft: 0xe3f1fb, text: 0x0369a1 }, // sky
    Pal { main: 0x65a30d, soft: 0xeef6e0, text: 0x4d7c0f }, // lime
];

/// Palette entry for a new project, round-robin by existing count.
pub fn nth_palette(i: usize) -> &'static Pal {
    &PROJECT_PALETTE[i % PROJECT_PALETTE.len()]
}

/// Look up the soft/text companions for a stored main color, else fall back.
pub fn pal_for_hex(hex: &str) -> &'static Pal {
    let m = hex_to_u32(hex);
    PROJECT_PALETTE
        .iter()
        .find(|p| p.main == m)
        .unwrap_or(&PROJECT_PALETTE[0])
}

pub fn hex_to_u32(s: &str) -> u32 {
    u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(ACCENT)
}

pub fn u32_to_hex(c: u32) -> String {
    format!("#{c:06x}")
}
