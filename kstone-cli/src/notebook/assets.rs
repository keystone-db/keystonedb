/// Embedded static assets for the notebook interface

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "src/notebook/static/"]
pub struct Asset;

/// Get an embedded asset by path
pub fn get_asset(path: &str) -> Option<Vec<u8>> {
    Asset::get(path).map(|file| file.data.to_vec())
}

/// List all available assets
pub fn list_assets() -> Vec<String> {
    Asset::iter().map(|path| path.to_string()).collect()
}