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
mod foliage;
mod gpu;
mod types;
mod viewer;

use anyhow::Result;
use std::path::PathBuf;

use viewer::TerrainViewer;
use xcore::app::WindowConfig;

fn main() -> Result<()> {
    // Load .env file if present (ignore errors if not found)
    dotenvy::dotenv_override()?;

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Terrain Viewer starting...");

    let args: Vec<String> = std::env::args().collect();

    let viewer = if args.len() > 1 {
        let arg = &args[1];
        if arg.ends_with(".xtd") {
            // File path mode: load from local XTD file
            log::info!("Loading terrain from file: {}", arg);
            TerrainViewer::new(Some(PathBuf::from(arg)))
        } else {
            // Scenario name mode: load from ERA archive
            log::info!("Loading scenario: {}", arg);
            TerrainViewer::from_scenario(arg.to_string())
        }
    } else {
        // Default to blood_gulch scenario
        log::info!("Loading default scenario: blood_gulch");
        TerrainViewer::from_scenario("blood_gulch".to_string())
    };

    let config = WindowConfig::new("Terrain Viewer - Halo Wars XTD", 1280, 720);
    render::run_3d(config, viewer)?;

    Ok(())
}
