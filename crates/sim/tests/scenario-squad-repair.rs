//! Checks that a Repair Work order (`BSquad_executeOrder` @ `0x140399780` case 4) heals and
//! reinforces a shipped Marine squad on Blood Gulch over the leader `RepairTime`.

use glam::Vec3;
use pipeline::database::hw1::Database;
use sim::{
    Command, CommandEntry, CommandExecutor, CommandPayload, EntityId, GameplayCatalog, OrderType,
    WorkCommand, World, spawn_squad_at, squad_prototype_id,
};

const MARINES: &str = "unsc_inf_marine_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-squad-repair -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn repair_order_heals_and_reinforces_a_shipped_marine_squad() {
    let mut loaded = test_support::installed_scenario("blood_gulch");
    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;
    let world = &mut loaded.simulation.world;
    let repair_time = use_leader_with_repair_time(world, database);
    let squad_id = spawn_marines(world, database);
    let members = world.get_squad(squad_id).unwrap().unit_ids.clone();
    assert!(members.len() >= 2, "the Marine squad has several members");
    let maximum = world.get_unit(members[0]).unwrap().max_hitpoints;
    assert!(world.damage_unit_direct(members[0], maximum * 0.5, 0.0));
    let _removed = world.remove_unit(members[1]).expect("the second Marine");
    let damaged = world.get_unit(members[0]).unwrap().hitpoints;

    execute_repair(world, database, squad_id);
    tick(world, database, gameplay);

    let healing = world.get_unit(members[0]).unwrap().hitpoints;
    assert!(healing > damaged, "the damaged Marine heals");
    assert!(healing < maximum, "the heal takes the repair time");
    assert_eq!(
        world.get_squad(squad_id).unwrap().unit_ids.len(),
        members.len() - 1,
        "the reinforcement comes after the heal"
    );

    let mut elapsed = 0.05;
    while elapsed <= repair_time + 0.1 {
        tick(world, database, gameplay);
        elapsed += 0.05;
    }

    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(
        squad.unit_ids.len(),
        members.len(),
        "the lost Marine returns"
    );
    test_support::assert_close(world.get_unit(members[0]).unwrap().hitpoints, maximum);
}

/// Player 1 gets the first leader with a `RepairTime`. Returns that time.
fn use_leader_with_repair_time(world: &mut World, database: &Database) -> f32 {
    let (index, time) = database
        .leaders
        .iter()
        .enumerate()
        .find_map(|(index, leader)| leader.repair_time.map(|time| (index, time)))
        .expect("a shipped leader has a RepairTime");
    assert!(time > 0.1, "the shipped RepairTime spans several updates");
    world.get_player_mut(1).unwrap().leader_id = i32::try_from(index).unwrap();
    time
}

fn spawn_marines(world: &mut World, database: &Database) -> EntityId {
    let bounds = world
        .terrain_bounds()
        .expect("authoritative terrain bounds");
    let mut position = Vec3::new(
        f32::midpoint(bounds.min_x(), bounds.max_x()),
        0.0,
        f32::midpoint(bounds.min_z(), bounds.max_z()),
    );
    position.y = world.terrain_height(position, true).unwrap_or_default();
    spawn_squad_at(
        world,
        database,
        1,
        squad_prototype_id(database, MARINES).expect("installed Marine squad"),
        position,
        Vec3::Z,
    )
    .expect("the Marines spawn from the layered scenario database")
}

fn execute_repair(world: &mut World, database: &Database, squad_id: EntityId) {
    let entry = CommandEntry {
        command: CommandPayload::Work(WorkCommand {
            base: Command {
                id: OrderType::Repair as i32,
                player_id: 1,
                recipients: vec![squad_id],
                ..Command::default()
            },
            ..WorkCommand::default()
        }),
        exec_time: 0,
        sequence: 0,
        source_client: 1,
    };
    CommandExecutor::with_database(database).execute(world, &entry);
}

fn tick(world: &mut World, database: &Database, gameplay: &GameplayCatalog) {
    world.advance_time(50);
    world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
}
