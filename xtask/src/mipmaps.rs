//! `cargo xtask mipmaps`: Android's launcher icons (spec E) — one mipmap PNG per density,
//! down-scaled from the app's own icon (`assets/app-icon/deckcraft-1024.png`, itself rendered
//! from `assets/app-icon/icon-source.deckcraft`) into the entry crate's `apps/deckcraft-android/res/`
//! tree. cargo-apk runs `aapt package -S res` over that tree, which turns these into the APK's
//! `@mipmap/ic_launcher`; aapt rejects a second resource name per directory, so every file must
//! be named exactly `ic_launcher.png`. Each PNG has its ATTRIBUTION.md row, like every asset.

use std::path::Path;

/// Each mipmap bucket and its icon size in pixels, as Android's launcher expects.
const DENSITIES: &[(&str, u32)] = &[("mdpi", 48), ("hdpi", 72), ("xhdpi", 96), ("xxhdpi", 144), ("xxxhdpi", 192)];

/// The source icon, square, from which every density is scaled.
pub const SOURCE: &str = "assets/app-icon/deckcraft-1024.png";

/// Render all five densities into `apps/deckcraft-android/res/mipmap-<density>/ic_launcher.png`.
pub fn run(root: &Path) -> Result<(), String> {
    let source = root.join(SOURCE);
    let img = image::open(&source).map_err(|e| format!("read {}: {e}", source.display()))?;
    if img.width() != img.height() {
        return Err(format!("the app icon must be square, got {}x{}", img.width(), img.height()));
    }
    for (density, size) in DENSITIES {
        let dir = root.join(format!("apps/deckcraft-android/res/mipmap-{density}"));
        std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        let out = dir.join("ic_launcher.png");
        // Lanczos3: the image crate's high-quality down-scaled filter, keeping the shapes legible
        // at 48 px. The files are committed, so builds and packaging never need this command.
        img.resize(*size, *size, image::imageops::FilterType::Lanczos3).save(&out).map_err(|e| format!("write {}: {e}", out.display()))?;
        println!("{}: {size} px", out.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ico::png_size;

    /// Every density lands on its exact size, named exactly `ic_launcher.png` (aapt's one-name
    /// rule), from the committed icon. Fails without the rendering.
    #[test]
    fn every_density_renders_at_its_exact_size() {
        let root = crate::root();
        let source = root.join(SOURCE);
        if !source.is_file() {
            return; // no committed icon on this checkout: nothing to render
        }
        // A scratch copy of the tree so the test never rewrites the committed res/ files.
        let tmp = std::env::temp_dir().join(format!("deckcraft-xtask-mipmaps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("assets/app-icon")).unwrap();
        std::fs::copy(&source, tmp.join(SOURCE)).unwrap();
        run(&tmp).unwrap();
        for (density, size) in DENSITIES {
            let out = tmp.join(format!("apps/deckcraft-android/res/mipmap-{density}/ic_launcher.png"));
            assert!(out.is_file(), "{density} has no ic_launcher.png");
            let png = std::fs::read(&out).unwrap();
            assert_eq!(png_size(&png).unwrap(), (*size, *size), "{density} must be {size} px square");
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
