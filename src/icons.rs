//! Typed handles for our vendored Lucide icons (`assets/icons/*.svg`), served by
//! the composite AssetSource in [`crate::assets`]. Use `Icon::new(Lucide::Clock)`
//! anywhere an icon is expected — the same call shape as gpui-component's
//! `IconName`, via the blanket `impl<T: IconNamed> From<T> for Icon`.
//!
//! To add an icon: drop `assets/icons/<name>.svg`, then add a variant + arm here.
//! (These are the Lucide glyphs the approved design uses that gpui-component's
//! bundle lacks; the rest — calendar, check, chevrons, plus, github,
//! external-link — come from `IconName` since that bundle already has them.)

use gpui::SharedString;
use gpui_component::IconNamed;

#[derive(Clone, Copy)]
pub enum Lucide {
    Clock,
    Layers,
    Download,
    FileText,
    Pencil,
}

impl IconNamed for Lucide {
    fn path(self) -> SharedString {
        match self {
            Lucide::Clock => "icons/clock.svg",
            Lucide::Layers => "icons/layers.svg",
            Lucide::Download => "icons/download.svg",
            Lucide::FileText => "icons/file-text.svg",
            Lucide::Pencil => "icons/pencil.svg",
        }
        .into()
    }
}
