use glam::Vec3;
use sim::{GroundMovePhase, load_scenario_from_game_dir, spawn_squad_at, squad_prototype_id};

const MARINES: &str = "unsc_inf_marine_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-ground-movement -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_marine_squad_uses_per_member_ground_move_actions() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load");
    let database = &loaded.content.database;
    let world = &mut loaded.simulation.world;
    let bounds = world
        .terrain_bounds()
        .expect("authoritative terrain bounds");
    let mut origin = Vec3::new(
        f32::midpoint(bounds.min_x(), bounds.max_x()),
        0.0,
        f32::midpoint(bounds.min_z(), bounds.max_z()),
    );
    origin.y = world.terrain_height(origin, true).unwrap_or_default();
    let mut target = origin + Vec3::X * 30.0;
    target.y = world.terrain_height(target, true).unwrap_or(origin.y);
    let squad_id = spawn_squad_at(
        world,
        database,
        1,
        squad_prototype_id(database, MARINES).expect("installed Marine squad"),
        origin,
        Vec3::Z,
    )
    .expect("Marine squad should spawn");
    let unit_ids = world.get_squad(squad_id).unwrap().unit_ids.clone();
    assert_eq!(unit_ids.len(), 4);
    assert!(world.issue_move_order(1, squad_id, target));

    world.update_entities_with_database(0.05, database);
    for &unit_id in &unit_ids {
        let unit = world.get_unit(unit_id).expect("live Marine member");
        assert_eq!(unit.ground_move_phase(), GroundMovePhase::Working);
        assert!(unit.ground_move_target().is_some());
        assert!(unit.has_active_move_action());
        assert!(unit.base.velocity.length() > 0.0);
    }

    let mut completed = false;
    for _ in 0..399 {
        world.update_entities_with_database(0.05, database);
        completed = world
            .get_squad(squad_id)
            .is_some_and(|squad| squad.base.position == target)
            && unit_ids.iter().all(|&unit_id| {
                world
                    .get_unit(unit_id)
                    .is_some_and(|unit| unit.ground_move_phase() == GroundMovePhase::Inactive)
            });
        if completed {
            break;
        }
    }
    if !completed {
        let squad = world.get_squad(squad_id).unwrap();
        let members = unit_ids
            .iter()
            .map(|&unit_id| {
                let unit = world.get_unit(unit_id).unwrap();
                (
                    unit_id,
                    unit.base.position,
                    unit.ground_move_phase(),
                    unit.ground_move_target(),
                )
            })
            .collect::<Vec<_>>();
        panic!(
            "Marine formation should converge: squad={:?} state={:?} target={:?} members={members:?}",
            squad.base.position, squad.state, squad.move_target,
        );
    }
    let squad = world.get_squad(squad_id).expect("arrived Marine squad");
    assert_eq!(squad.base.position, target);
    for &unit_id in &unit_ids {
        let unit = world.get_unit(unit_id).expect("arrived Marine member");
        assert_eq!(unit.ground_move_phase(), GroundMovePhase::Inactive);
        assert_eq!(unit.base.velocity, Vec3::ZERO);
        assert_ne!(unit.base.position, squad.base.position);
    }
}
