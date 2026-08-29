pub(super) fn squad_member_hitpoints(world: &sim::World, member_ids: &[sim::EntityId]) -> f32 {
    member_ids
        .iter()
        .filter_map(|&unit_id| world.get_unit(unit_id))
        .map(|unit| unit.hitpoints)
        .sum()
}

pub(super) fn squad_member_hitpoint_snapshot(
    world: &sim::World,
    squad_id: sim::EntityId,
) -> (Vec<sim::EntityId>, Vec<f32>) {
    let member_ids = world
        .get_squad(squad_id)
        .expect("rocket target squad should be alive")
        .unit_ids
        .clone();
    let hitpoints = squad_member_hitpoint_values(world, &member_ids);
    (member_ids, hitpoints)
}

pub(super) fn damaged_squad_member_count(
    world: &sim::World,
    member_ids: &[sim::EntityId],
    initial_hitpoints: &[f32],
) -> usize {
    initial_hitpoints
        .iter()
        .zip(squad_member_hitpoint_values(world, member_ids))
        .filter(|(initial, current)| current < *initial)
        .count()
}

fn squad_member_hitpoint_values(world: &sim::World, member_ids: &[sim::EntityId]) -> Vec<f32> {
    member_ids
        .iter()
        .map(|&unit_id| world.get_unit(unit_id).map_or(0.0, |unit| unit.hitpoints))
        .collect()
}
