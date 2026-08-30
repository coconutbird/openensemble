use std::collections::VecDeque;

#[test]
#[ignore = "requires an installed Halo Wars DE data directory"]
fn reports_blood_gulch_static_alpha_components() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut loaded =
        sim::load_scenario_from_game_dir(&game_dir, "blood_gulch").expect("load Blood Gulch");
    for (&player_id, &base_id) in &loaded.simulation.initial_base_ids {
        let base = loaded
            .simulation
            .world
            .get_base(base_id)
            .expect("initial base");
        let anchor = loaded
            .simulation
            .world
            .get_building(base.anchor_building_id)
            .expect("initial base anchor");
        println!(
            "player {player_id} base {:?} anchor={} position={:?} forward={:?}",
            base.position, anchor.proto_object_name, anchor.base.position, anchor.base.forward
        );
    }
    for (_, unit) in loaded.simulation.world.units.iter() {
        let position = unit.base.position;
        if position.distance(glam::Vec3::new(151.0, position.y, 535.0)) < 48.0
            || position.distance(glam::Vec3::new(740.0, position.y, 394.0)) < 48.0
        {
            println!(
                "near alpha: id={:?} proto={} position={:?} forward={:?}",
                unit.base.id, unit.proto_object_name, position, unit.base.forward
            );
        }
    }
    let terrain = loaded
        .content
        .terrain_data
        .take()
        .expect("Blood Gulch terrain data");
    let raw = terrain.extract_raw_data().expect("extract packed terrain");
    let tessellation = terrain
        .decode_tessellation()
        .expect("decode terrain tessellation")
        .expect("Blood Gulch tessellation data");
    let alpha = terrain.decode_alpha().expect("decode static terrain alpha");
    let components = low_alpha_components(&alpha.values, alpha.width, alpha.height, 169);

    println!("static alpha dimensions: {}x{}", alpha.width, alpha.height);
    println!(
        "tile scale={} world min={:?} world max={:?}",
        raw.tile_scale, raw.world_min, raw.world_max
    );
    println!("low-alpha components: {}", components.len());
    for component in &components {
        println!(
            "  size={} bounds=({}, {})-({}, {}) min={} max={}",
            component.size,
            component.min_x,
            component.min_y,
            component.max_x,
            component.max_y,
            component.min_value,
            component.max_value,
        );
        let world_patch_x = (component.min_y + component.max_y) / 32;
        let world_patch_z = (component.min_x + component.max_x) / 32;
        let world_patch_x = i32::try_from(world_patch_x).expect("patch x fits i32");
        let world_patch_z = i32::try_from(world_patch_z).expect("patch z fits i32");
        println!(
            "    world patch=({}, {}) source level={:?}",
            world_patch_x,
            world_patch_z,
            tessellation.get_patch_tess_level(world_patch_z, world_patch_x),
        );
    }
}

#[derive(Debug)]
struct Component {
    size: usize,
    min_x: usize,
    min_y: usize,
    max_x: usize,
    max_y: usize,
    min_value: u8,
    max_value: u8,
}

fn low_alpha_components(
    values: &[u8],
    width: usize,
    height: usize,
    threshold: u8,
) -> Vec<Component> {
    assert_eq!(values.len(), width * height);
    let mut visited = vec![false; values.len()];
    let mut components = Vec::new();

    for start in 0..values.len() {
        if visited[start] || values[start] > threshold {
            continue;
        }
        let mut queue = VecDeque::from([start]);
        visited[start] = true;
        let mut component = Component {
            size: 0,
            min_x: width,
            min_y: height,
            max_x: 0,
            max_y: 0,
            min_value: u8::MAX,
            max_value: u8::MIN,
        };

        while let Some(index) = queue.pop_front() {
            let x = index % width;
            let y = index / width;
            component.size += 1;
            component.min_x = component.min_x.min(x);
            component.min_y = component.min_y.min(y);
            component.max_x = component.max_x.max(x);
            component.max_y = component.max_y.max(y);
            component.min_value = component.min_value.min(values[index]);
            component.max_value = component.max_value.max(values[index]);

            for neighbor in neighbors(x, y, width, height) {
                if !visited[neighbor] && values[neighbor] <= threshold {
                    visited[neighbor] = true;
                    queue.push_back(neighbor);
                }
            }
        }
        components.push(component);
    }
    components
}

fn neighbors(x: usize, y: usize, width: usize, height: usize) -> impl Iterator<Item = usize> {
    let left = (x > 0).then_some(y * width + x.saturating_sub(1));
    let right = (x + 1 < width).then_some(y * width + x + 1);
    let up = (y > 0).then_some(y.saturating_sub(1) * width + x);
    let down = (y + 1 < height).then_some((y + 1) * width + x);
    [left, right, up, down].into_iter().flatten()
}
