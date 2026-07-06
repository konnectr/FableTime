//! Composite asset source: serves our own vendored Lucide SVGs (`assets/icons`)
//! first, then falls back to gpui-component's bundled icon set. Both live under
//! the `icons/<name>.svg` path namespace, so `Icon::path("icons/clock.svg")`
//! resolves whether the icon is ours or theirs.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

/// Our vendored icons, embedded from `assets/icons/**/*.svg` at build time.
#[derive(rust_embed::RustEmbed)]
#[folder = "assets"]
#[include = "icons/**/*.svg"]
struct LocalIcons;

/// The app's asset source: our icons take precedence, gpui-component's bundle
/// (chevrons, checks, etc. used internally by components) is the fallback.
pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }
        if let Some(f) = LocalIcons::get(path) {
            return Ok(Some(f.data));
        }
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut out = gpui_component_assets::Assets.list(path)?;
        out.extend(LocalIcons::iter().filter(|p| p.starts_with(path)).map(Into::into));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::Lucide;
    use gpui_component::IconNamed;

    #[test]
    fn vendored_lucide_icons_resolve() {
        let assets = AppAssets;
        for ic in [
            Lucide::Clock,
            Lucide::Layers,
            Lucide::Download,
            Lucide::FileText,
            Lucide::Pencil,
        ] {
            let path = ic.path();
            assert!(
                assets.load(&path).unwrap().is_some(),
                "vendored asset should resolve: {path}"
            );
        }
        // Fallback into gpui-component's bundle still works.
        assert!(assets.load("icons/check.svg").unwrap().is_some());
        assert!(assets.load("icons/does-not-exist.svg").is_err());
    }
}
