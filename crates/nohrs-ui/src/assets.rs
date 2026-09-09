use rust_embed::RustEmbed;

// Crate-local assets so `nohrs-ui` stays self-contained and publishable: the
// folder is resolved relative to this crate's manifest dir and is included in
// the packaged crate. Workspace-root `assets/doc/` (README images) is separate.
/// GPUI `AssetSource` backed by the crate-local `assets/` directory embedded at compile time.
#[derive(RustEmbed)]
#[folder = "assets/"]
pub struct Assets;

impl gpui::AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        Self::get(path)
            .map(|f| Some(f.data))
            .ok_or_else(|| anyhow::anyhow!("could not find asset at path \"{}\"", path))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<gpui::SharedString>> {
        Ok(Self::iter()
            .filter_map(|p| {
                if p.starts_with(path) {
                    Some(gpui::SharedString::from(p.to_string()))
                } else {
                    None
                }
            })
            .collect())
    }
}
