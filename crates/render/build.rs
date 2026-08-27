use wesl::Wesl;

fn main() {
    let compiler = Wesl::new("src/terrain/shaders");

    // Compile each WESL entry point to WGSL in OUT_DIR.
    for name in &[
        "terrain_gpu",
        "terrain_heightfield",
        "terrain_roads",
        "foliage",
        "terrain_composite",
        "shadow_depth",
    ] {
        compiler.build_artifact(&format!("package::{name}").parse().unwrap(), name);
    }
}
