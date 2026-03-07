use wesl::Wesl;

fn main() {
    let compiler = Wesl::new("src/terrain/shaders");

    // Compile each WESL entry point to WGSL in OUT_DIR.
    // Plain .wgsl files (terrain) are loaded directly via include_str!.
    for name in &[
        "gpu_tess",
        "terrain_heightfield",
        "terrain_roads",
        "foliage",
        "composite",
    ] {
        compiler.build_artifact(&format!("package::{name}").parse().unwrap(), name);
    }
}
