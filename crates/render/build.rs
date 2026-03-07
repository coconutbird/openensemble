use wesl::Wesl;

fn main() {
    let compiler = Wesl::new("src/terrain/shaders");

    // Compile each WESL entry point to WGSL in OUT_DIR.
    // Plain .wgsl files (terrain, composite, foliage) are loaded directly
    // via include_str! and don't need WESL compilation.
    for name in &["gpu_tess", "terrain_heightfield", "terrain_roads"] {
        compiler.build_artifact(&format!("package::{name}").parse().unwrap(), name);
    }
}
