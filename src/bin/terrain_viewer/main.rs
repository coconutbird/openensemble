//! Terrain Viewer for Halo Wars XTD files.
//!
//! Loads and renders terrain from XTD files using wgpu.
//! Supports XTT texturing for albedo atlas rendering.
//! WASD + mouse to fly around the terrain.
//!
//! ## Missing Rendering Features (TODO)
//!
//! Texture quality improvements needed:
//! - [x] Mipmaps - generate mipmaps for splat textures (fixes distance aliasing)
//! - [ ] Normal maps - load `_nm.ddx` files alongside `_df.ddx` for surface detail
//! - [ ] Specular maps - add specular lighting (Blinn-Phong or PBR)
//!
//! Terrain features:
//! - [ ] Alpha chunk (0xDDDD) - terrain holes/transparency, same compression as AO
//! - [ ] Lighting chunk (0xBBBB) - baked lightmap (empty on blood_gulch)
//! - [ ] Decals - road marks, scorch marks from XTT linker decal data
//!
//! Visual effects:
//! - [ ] Fog - atmospheric depth
//! - [ ] Environment reflections - gUniqueEnvMaskTexture
//! - [ ] Dynamic shadows

mod camera;
mod gpu;
mod types;
mod viewer;

use anyhow::Result;
use std::path::PathBuf;

use viewer::TerrainViewer;
use xcore::app::WindowConfig;

fn main() -> Result<()> {
    // Load .env file if present (ignore errors if not found)
    let _ = dotenvy::dotenv();

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Terrain Viewer starting...");

    // Parse args for XTD path
    let args: Vec<String> = std::env::args().collect();
    let xtd_path = if args.len() > 1 {
        Some(PathBuf::from(&args[1]))
    } else {
        // Default to test file
        let default_path = PathBuf::from(
            "../ensemble-rs/test_extract/scenario/skirmish/design/blood_gulch/blood_gulch.xtd",
        );
        if default_path.exists() {
            Some(default_path)
        } else {
            None
        }
    };

    if let Some(path) = &xtd_path {
        log::info!("XTD file: {}", path.display());
    } else {
        log::warn!("No XTD file specified. Usage: terrain_viewer <path/to/file.xtd>");
    }

    let config = WindowConfig::new("Terrain Viewer - Halo Wars XTD", 1280, 720);
    render::run_3d(config, TerrainViewer::new(xtd_path))?;

    Ok(())
}
