use glam::Vec3;

use super::HeightFieldRayHit;
use crate::heightfield::HeightField;

const SIDE: usize = 8;

/// Makes an 8 × 8 grid with tile scale 1 from a height function of the sim vertex.
fn field(height: impl Fn(usize, usize) -> f32) -> HeightField {
    let heights = (0..SIDE * SIDE)
        .map(|index| height(index % SIDE, index / SIDE))
        .collect();
    HeightField::new(SIDE, 1.0, heights).expect("valid grid")
}

/// Makes a grid that is flat at 0 with one ridge of height 4 on each listed sim X column.
fn ridges(columns: &[usize]) -> HeightField {
    field(|x, _| if columns.contains(&x) { 4.0 } else { 0.0 })
}

/// A grid with a different height at most vertices, so each triangle has its own slope.
fn bumps() -> HeightField {
    field(|x, z| {
        let mixed = (x * 7 + z * 13 + x * z) % 5;
        f32::from(u8::try_from(mixed).expect("small")) * 0.8
    })
}

/// Casts a ray between two world points with the terrain body at its retail position.
fn cast(field: &HeightField, from: Vec3, to: Vec3) -> Option<HeightFieldRayHit> {
    let origin = field.body_position();
    field.cast_ray_local(from - origin, to - origin)
}

/// Returns the vertical gap between a world point and the surface, or `None` outside the grid.
fn gap(field: &HeightField, point: Vec3) -> Option<f32> {
    field
        .sample(point - field.body_position())
        .map(|surface| point.y - surface.height)
}

/// Finds the first point where the ray goes from above to below the surface by dense
/// samples and a bisection. A start below the surface or outside the grid does not count.
fn reference_fraction(field: &HeightField, from: Vec3, to: Vec3) -> Option<f32> {
    const STEPS: u16 = 20_000;
    let at = |t: f32| gap(field, from + (to - from) * t);
    let mut above = false;
    let mut last = 0.0;
    for step in 0..=STEPS {
        let t = f32::from(step) / f32::from(STEPS);
        match at(t) {
            Some(distance) if distance < 0.0 => {
                if above {
                    let (mut low, mut high) = (last, t);
                    for _ in 0..40 {
                        let middle = f32::midpoint(low, high);
                        if at(middle).is_some_and(|distance| distance < 0.0) {
                            high = middle;
                        } else {
                            low = middle;
                        }
                    }
                    return Some(f32::midpoint(low, high));
                }
            }
            Some(_) => above = true,
            None => above = false,
        }
        last = t;
    }
    None
}

fn assert_near(actual: f32, expected: f32, tolerance: f32) {
    assert!(
        (actual - expected).abs() < tolerance,
        "{actual} is not near {expected}"
    );
}

fn assert_normal(hit: HeightFieldRayHit, expected: Vec3) {
    let expected = expected.normalize();
    assert!(
        (hit.normal - expected).length() < 1e-5,
        "{hit:?} does not have the normal {expected}"
    );
}

#[test]
fn a_horizontal_ray_across_cells_hits_the_first_ramp() {
    // The ramp from x = 4 (height 0) to x = 5 (height 4) is at height 2 at x = 4.5. The ray
    // crosses three flat cells first and does not reach the second ramp.
    let field = ridges(&[5]);
    let hit = cast(&field, Vec3::new(1.0, 2.0, 3.5), Vec3::new(7.0, 2.0, 3.5)).expect("hit");
    assert_near(hit.fraction, 3.5 / 6.0, 1e-5);
    assert_normal(hit, Vec3::new(-4.0, 1.0, 0.0));
}

#[test]
fn a_horizontal_ray_along_z_hits_the_ramp_across_its_path() {
    let field = field(|_, z| if z == 5 { 4.0 } else { 0.0 });
    let hit = cast(&field, Vec3::new(3.5, 2.0, 1.0), Vec3::new(3.5, 2.0, 7.0)).expect("hit");
    assert_near(hit.fraction, 3.5 / 6.0, 1e-5);
    assert_normal(hit, Vec3::new(0.0, 1.0, -4.0));
}

#[test]
fn rays_in_all_directions_hit_the_first_surface_point_of_a_dense_sample() {
    let field = bumps();
    let directions = [
        Vec3::new(5.0, 0.0, 1.5),
        Vec3::new(1.5, 0.0, 5.0),
        Vec3::new(-5.0, 0.0, 1.5),
        Vec3::new(-1.5, 0.0, 5.0),
        Vec3::new(5.0, 0.0, -1.5),
        Vec3::new(1.5, 0.0, -5.0),
        Vec3::new(-5.0, 0.0, -1.5),
        Vec3::new(-1.5, 0.0, -5.0),
        Vec3::new(4.0, 0.0, 3.7),
        Vec3::new(-4.0, 0.0, 4.2),
        Vec3::new(3.3, 0.0, -4.1),
        Vec3::new(-4.4, 0.0, -3.9),
    ];
    let mut hits = 0;
    for direction in directions {
        let from = Vec3::new(3.6, 3.5, 3.3) - direction * 0.5;
        let to = from + direction + Vec3::new(0.0, -4.0, 0.0);
        let expected = reference_fraction(&field, from, to);
        let hit = cast(&field, from, to);
        match (hit, expected) {
            (Some(hit), Some(expected)) => {
                hits += 1;
                assert_near(hit.fraction, expected, 1e-4);
                // The normal belongs to the triangle just before the hit point.
                let before = from + (to - from) * (hit.fraction - 1e-3);
                let surface = field
                    .sample(before - field.body_position())
                    .expect("inside");
                assert_normal(hit, surface.normal);
            }
            (None, None) => {}
            _ => panic!("{direction}: {hit:?} against {expected:?}"),
        }
    }
    assert_eq!(hits, directions.len());
}

#[test]
fn a_ray_parallel_to_the_diagonal_keeps_the_retail_crossing_order() {
    // With dx = dz the walk has no diagonal crossings. Retail then takes the X and Z crossings
    // in turn and starts with X, even where the Z crossing comes first. The ray from sim
    // (1.6, 1.3) visits X at t = -0.15, Z at t = 0.175 (ray 2.8, surface 2.24) and X at
    // t = 0.1 (ray 3.1, surface 3.2). The line between the last two gives the hit. The true
    // first contact is at t = 0.0926.
    let field = bumps();
    let from = Vec3::new(1.6, 3.5, 1.3);
    let to = Vec3::new(5.6, -0.5, 5.3);
    let hit = cast(&field, from, to).expect("hit");
    let previous: f32 = 2.8 - 2.24;
    let expected = previous / (previous + 0.1) * (0.1 - 0.175) + 0.175;
    assert_near(hit.fraction, expected, 1e-5);
    assert_normal(hit, Vec3::new(-3.2, 1.0, 0.0));
    let reference = reference_fraction(&field, from, to).expect("hit");
    assert_near(reference, 0.092_647, 1e-4);
}

#[test]
fn a_ray_above_the_terrain_or_outside_the_grid_misses() {
    let field = ridges(&[5]);
    let above = cast(&field, Vec3::new(1.0, 4.5, 3.5), Vec3::new(7.0, 4.5, 3.5));
    assert_eq!(above, None);
    // The ray stops before it reaches the ramp.
    let short = cast(&field, Vec3::new(1.0, 2.0, 3.5), Vec3::new(4.4, 2.0, 3.5));
    assert_eq!(short, None);
    // The ray goes past the far edge of the skirt, away from the grid.
    let outside = cast(
        &field,
        Vec3::new(20.0, 5.0, 3.5),
        Vec3::new(24.0, -5.0, 3.5),
    );
    assert_eq!(outside, None);
    let vertical_outside = cast(
        &field,
        Vec3::new(3.5, 5.0, -12.0),
        Vec3::new(3.5, -5.0, -12.0),
    );
    assert_eq!(vertical_outside, None);
}

#[test]
fn a_ray_that_goes_up_through_the_surface_misses() {
    let field = bumps();
    let up = cast(&field, Vec3::new(3.6, -2.0, 3.3), Vec3::new(3.6, 8.0, 3.3));
    assert_eq!(up, None);
    let slanted = cast(&field, Vec3::new(2.1, -1.0, 3.3), Vec3::new(2.4, 8.0, 3.6));
    assert_eq!(slanted, None);
}

#[test]
fn a_ray_that_starts_below_the_surface_hits_only_after_it_comes_out() {
    // A vertical ray only tests the start triangle, so a start below gives no hit.
    let field = ridges(&[3, 6]);
    let vertical = cast(&field, Vec3::new(3.2, 3.0, 3.5), Vec3::new(3.2, -5.0, 3.5));
    assert_eq!(vertical, None);
    // The ray starts below the ramp down from x = 3 (height 3.2 at x = 3.2), comes out at
    // x = 3.25 and hits the ramp up to x = 6 at x = 5.75.
    let hit = cast(&field, Vec3::new(3.2, 3.0, 3.5), Vec3::new(7.0, 3.0, 3.5)).expect("hit");
    assert_near(hit.fraction, (5.75 - 3.2) / 3.8, 1e-5);
    assert_normal(hit, Vec3::new(-4.0, 1.0, 0.0));
    // The same ray stops before the second ramp.
    let short = cast(&field, Vec3::new(3.2, 3.0, 3.5), Vec3::new(5.5, 3.0, 3.5));
    assert_eq!(short, None);
}

#[test]
fn a_vertical_ray_hits_the_triangle_on_the_far_side_of_the_diagonal() {
    // The cell of the shared test has h00 = 0, h10 = 2, h01 = 6 and h11 = 4. The point
    // (0.2, 0.8) of the cell lies on the triangle h00–h01–h11 at height 6 · 0.8 - 2 · 0.2.
    let mut heights = vec![0.0; 9];
    heights[1] = 2.0;
    heights[3] = 6.0;
    heights[4] = 4.0;
    let field = HeightField::new(3, 2.0, heights).expect("grid");
    let hit = cast(
        &field,
        Vec3::new(0.4, 10.0, 1.6),
        Vec3::new(0.4, -10.0, 1.6),
    )
    .expect("hit");
    assert_near(hit.fraction, (10.0 - 4.4) / 20.0, 1e-5);
    assert_normal(hit, Vec3::new(1.0, 1.0, -3.0));
}

#[test]
fn the_skirt_repeats_the_edge_heights() {
    let field = field(|x, z| if x == 0 || z == 0 || z == 7 { 2.0 } else { 0.0 });
    let vertical = cast(
        &field,
        Vec3::new(-5.0, 10.0, 3.0),
        Vec3::new(-5.0, -10.0, 3.0),
    )
    .expect("hit");
    assert_near(vertical.fraction, 0.4, 1e-6);
    assert_normal(vertical, Vec3::Y);
    // The ray starts outside the skirt, enters the shape at x = -10 at height 2.5 and meets the
    // skirt plateau at height 2 at x = -9.
    let entering = cast(
        &field,
        Vec3::new(-15.0, 5.0, 3.0),
        Vec3::new(-5.0, 0.0, 3.0),
    )
    .expect("hit");
    assert_near(entering.fraction, 0.6, 1e-5);
    assert_normal(entering, Vec3::Y);
    // A ray that ends before the skirt does not hit.
    let before = cast(
        &field,
        Vec3::new(-15.0, 5.0, 3.0),
        Vec3::new(-11.0, -5.0, 3.0),
    );
    assert_eq!(before, None);
    // The ray enters the shape over the far Z edge (world z = 17) at height 3 and meets the
    // skirt plateau at height 2 at z = 16.
    let far = cast(
        &field,
        Vec3::new(3.0, 11.0, 25.0),
        Vec3::new(3.0, 1.0, 15.0),
    )
    .expect("hit");
    assert_near(far.fraction, 0.9, 1e-5);
    assert_normal(far, Vec3::Y);
}

#[test]
fn the_same_ray_gives_the_same_bits() {
    let field = bumps();
    let copy = field.clone();
    let from = Vec3::new(0.3, 3.5, 6.7);
    let to = Vec3::new(6.9, -0.5, 0.2);
    let bits = |hit: HeightFieldRayHit| {
        [
            hit.fraction.to_bits(),
            hit.normal.x.to_bits(),
            hit.normal.y.to_bits(),
            hit.normal.z.to_bits(),
        ]
    };
    let first = cast(&field, from, to).expect("hit");
    let second = cast(&field, from, to).expect("hit");
    let other = cast(&copy, from, to).expect("hit");
    assert_eq!(bits(first), bits(second));
    assert_eq!(bits(first), bits(other));
}
