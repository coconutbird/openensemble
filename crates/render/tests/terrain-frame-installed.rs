//! Checks that the render terrain of `blood_gulch` uses the retail world frame.
//!
//! The scenario `.scn` positions and the simulation XSD terrain use the same frame. The render
//! terrain mesh from the XTD must give the same height at the same world X and Z.
//!
//! The test needs a local Halo Wars DE installation.

use glam::{Vec2, Vec3};
use num_traits::ToPrimitive;
use render::terrain::{TerrainMesh, TerrainScene};

/// Authored `.scn` positions `(name, x, y, z)` of objects that sit on the ground.
const AUTHORED_GROUND_OBJECTS: [(&str, f32, f32, f32); 4] = [
    ("reactor", 729.749, 38.81, 480.494),
    ("sniper platform", 206.1033, 38.70, 630.152),
    ("base socket", 513.0684, 0.28, 662.3633),
    ("crate", 929.0198, 6.55, 751.3801),
];

/// Largest permitted height difference at one authored object position.
const AUTHORED_HEIGHT_TOLERANCE: f32 = 1.5;

/// Distance between two sample points of the comparison with the simulation terrain.
const SIMULATION_SAMPLE_STEP: f32 = 23.0;

/// Width of one square bucket of the triangle search, in world units.
const BUCKET_SIZE: f32 = 8.0;

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn blood_gulch_render_terrain_uses_the_scenario_frame() {
    let loaded = test_support::installed_scenario("blood_gulch");
    let terrain = loaded
        .content
        .terrain_data
        .as_ref()
        .expect("Blood Gulch terrain data");
    let scene = TerrainScene::load(terrain, None, None).expect("load the terrain scene");
    let heights = MeshHeights::new(&scene.mesh);
    let mesh = &scene.mesh;

    for (name, x, authored_y, z) in AUTHORED_GROUND_OBJECTS {
        let height = heights
            .height(Vec2::new(x, z))
            .unwrap_or_else(|| panic!("no up-facing terrain triangle below the {name}"));
        println!("{name}: authored y {authored_y:.2}, render terrain y {height:.2}");
        assert!(
            (height - authored_y).abs() <= AUTHORED_HEIGHT_TOLERANCE,
            "render terrain height {height:.2} below the {name} is not near its authored y \
             {authored_y:.2}"
        );
    }

    let world = &loaded.simulation.world;
    let extent = mesh.world_max[0].min(mesh.world_max[2]);
    let mut sample_count = 0_u32;
    let mut total_difference = 0.0_f32;
    let mut near_count = 0_u32;
    let mut x = SIMULATION_SAMPLE_STEP;
    while x < extent - SIMULATION_SAMPLE_STEP {
        let mut z = SIMULATION_SAMPLE_STEP;
        while z < extent - SIMULATION_SAMPLE_STEP {
            let render_height = heights
                .height(Vec2::new(x, z))
                .unwrap_or_else(|| panic!("no up-facing terrain triangle at ({x}, {z})"));
            let simulation_height = world
                .terrain_height(Vec3::new(x, 0.0, z), true)
                .expect("simulation terrain height");
            let difference = (render_height - simulation_height).abs();
            total_difference += difference;
            sample_count += 1;
            if difference <= AUTHORED_HEIGHT_TOLERANCE {
                near_count += 1;
            }
            z += SIMULATION_SAMPLE_STEP;
        }
        x += SIMULATION_SAMPLE_STEP;
    }
    let samples = f64::from(sample_count);
    let mean_difference = f64::from(total_difference) / samples;
    let near_fraction = f64::from(near_count) / samples;
    println!(
        "render and simulation terrain: {sample_count} samples, mean difference \
         {mean_difference:.3}, near fraction {near_fraction:.3}"
    );
    // The simulation XSD heights and the XTD heights differ near cliffs by more than the
    // tolerance. With the X and Z axes swapped, the mean difference is about 13.
    assert!(
        mean_difference < 1.0,
        "mean render and simulation height difference {mean_difference:.3} is too large"
    );
    assert!(
        near_fraction > 0.9,
        "only {near_fraction:.3} of the render heights are near the simulation heights"
    );
}

/// Height lookup in the X/Z plane that uses only the mesh vertex positions and triangle
/// indices. It does not use the order of the vertices in the grid.
struct MeshHeights<'a> {
    mesh: &'a TerrainMesh,
    buckets_per_axis: usize,
    /// First index of each triangle that overlaps a bucket, for each bucket.
    buckets: Vec<Vec<usize>>,
}

impl<'a> MeshHeights<'a> {
    fn new(mesh: &'a TerrainMesh) -> Self {
        let extent = mesh.world_max[0].max(mesh.world_max[2]) + BUCKET_SIZE;
        let buckets_per_axis = bucket_coordinate(extent) + 1;
        let mut buckets = vec![Vec::new(); buckets_per_axis * buckets_per_axis];
        for first in (0..mesh.indices.len()).step_by(3) {
            let [a, b, c] = triangle(mesh, first).map(|vertex| Vec2::new(vertex.x, vertex.z));
            let minimum = a.min(b).min(c);
            let maximum = a.max(b).max(c);
            for bucket_x in bucket_coordinate(minimum.x)..=bucket_coordinate(maximum.x) {
                for bucket_z in bucket_coordinate(minimum.y)..=bucket_coordinate(maximum.y) {
                    if bucket_x < buckets_per_axis && bucket_z < buckets_per_axis {
                        buckets[bucket_z * buckets_per_axis + bucket_x].push(first);
                    }
                }
            }
        }
        Self {
            mesh,
            buckets_per_axis,
            buckets,
        }
    }

    /// Returns the height of the up-facing triangle that contains `point` in the X/Z plane.
    fn height(&self, point: Vec2) -> Option<f32> {
        let bucket_x = bucket_coordinate(point.x);
        let bucket_z = bucket_coordinate(point.y);
        let bucket = self
            .buckets
            .get(bucket_z * self.buckets_per_axis + bucket_x)?;
        bucket.iter().find_map(|&first| {
            let [a, b, c] = triangle(self.mesh, first);
            let [a_xz, b_xz, c_xz] = [a, b, c].map(|vertex| Vec2::new(vertex.x, vertex.z));
            let up_facing = (b - a).cross(c - a).y > 0.0;
            let area = (b_xz - a_xz).perp_dot(c_xz - a_xz);
            if area.abs() <= f32::EPSILON {
                return None;
            }
            let weight_b = (point - a_xz).perp_dot(c_xz - a_xz) / area;
            let weight_c = (b_xz - a_xz).perp_dot(point - a_xz) / area;
            let weight_a = 1.0 - weight_b - weight_c;
            let inside = [weight_a, weight_b, weight_c]
                .iter()
                .all(|weight| *weight >= -1.0e-4);
            (inside && up_facing).then_some(weight_a * a.y + weight_b * b.y + weight_c * c.y)
        })
    }
}

fn triangle(mesh: &TerrainMesh, first: usize) -> [Vec3; 3] {
    [0, 1, 2].map(|corner| {
        let index = usize::try_from(mesh.indices[first + corner]).expect("vertex index");
        Vec3::from_array(mesh.positions[index])
    })
}

fn bucket_coordinate(world: f32) -> usize {
    (world.max(0.0) / BUCKET_SIZE)
        .floor()
        .to_usize()
        .expect("bucket coordinate")
}
