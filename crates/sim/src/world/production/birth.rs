//! Trainer-owned containment, queued birth, animation, and rally release.

use super::World;
use super::placement::{SquadBirthPlacement, effective_squad_prototype, trained_squad_placement};
use crate::entities::{RallyPoint, SquadState, UnitState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::GameplayCatalog;
use crate::player::PlayerId;
use glam::Vec3;
use num_traits::ToPrimitive;
use pipeline::database::hw1::squads::Birth;
use pipeline::database::hw1::{Database, ProtoObject};
use std::collections::BTreeSet;

const DIRECT_RALLY_CLEARANCE: f32 = 4.0;
const PLOTTER_SQUAD_SPACING: f32 = 3.0;

#[derive(Debug, Clone)]
struct BirthProfile {
    member_animation: Option<String>,
    trainer_animation: Option<String>,
    apply_formation: bool,
    fly_in: bool,
    force_delay: bool,
}

impl World {
    pub(super) fn training_birth_origin(
        &self,
        building_id: EntityId,
    ) -> Option<(EntityId, Vec3, Vec3)> {
        let controller_id = self
            .get_building(building_id)?
            .associated_parking_lot()
            .filter(|parking_lot_id| self.get_building(*parking_lot_id).is_some())
            .unwrap_or(building_id);
        let building = self.get_building(controller_id)?;
        Some((
            controller_id,
            building.base.position,
            planar_forward(building.base.forward),
        ))
    }

    pub(super) fn contain_and_queue_trained_squad(
        &mut self,
        building_id: EntityId,
        squad_id: EntityId,
        play_sound: bool,
    ) -> bool {
        let Some((position, forward, container_squad_id, unit_ids)) =
            self.trained_squad_containment_snapshot(building_id, squad_id)
        else {
            return false;
        };
        for unit_id in &unit_ids {
            if let Some(unit) = self.get_unit_mut(*unit_id) {
                unit.garrison.set_container(Some(building_id));
                unit.stop();
                unit.state = UnitState::Idle;
            }
            if let Some(building) = self.get_building_mut(building_id) {
                building.garrison.add_contained_unit(*unit_id);
            }
        }
        if let Some(container_squad_id) = container_squad_id
            && let Some(container_squad) = self.get_squad_mut(container_squad_id)
        {
            container_squad.garrison.add_contained_squad(squad_id);
        }
        let now_ms = self.game_time_ms;
        if let Some(squad) = self.get_squad_mut(squad_id) {
            squad.remove_all_orders();
            squad.base.position = position;
            squad.base.forward = forward;
            squad.base.velocity = Vec3::ZERO;
            squad.state = SquadState::Idle;
            squad.garrison.mark_garrisoned(building_id, now_ms);
        }
        self.place_squad_members(squad_id, position, forward, false);
        if let Some(building) = self.get_building_mut(building_id) {
            building
                .production
                .queue_trained_squad(squad_id, play_sound);
            true
        } else {
            false
        }
    }

    pub(super) fn update_trained_squad_birth(
        &mut self,
        building_id: EntityId,
        dt: f32,
        database: &Database,
        gameplay: Option<&GameplayCatalog>,
    ) {
        self.remove_missing_trained_births(building_id);
        let ready = self
            .get_building_mut(building_id)
            .is_some_and(|building| building.production.advance_trained_squad_birth_time(dt));
        if !ready {
            return;
        }
        let Some((squad_id, player_id)) = self
            .get_building(building_id)
            .and_then(|building| building.production.first_trained_squad())
            .and_then(|birth| {
                self.get_squad(birth.squad_id())
                    .map(|squad| (birth.squad_id(), squad.base.player_id))
            })
        else {
            return;
        };
        let producer_id = self.trained_squad_producer(squad_id).unwrap_or(building_id);
        self.plot_direct_training_rallies(building_id, producer_id, player_id);
        let Some(birth) = self
            .get_building(building_id)
            .and_then(|building| building.production.first_trained_squad())
        else {
            return;
        };
        let Some(placement) = trained_squad_placement(self, building_id, squad_id, database) else {
            return;
        };
        let rally = self.trained_birth_rally_point(building_id, producer_id, player_id);
        let profile = self.birth_profile(building_id, squad_id, database);
        self.release_trained_squad(squad_id, placement);
        let rally_destination = birth
            .plotted_position()
            .or_else(|| rally.map(|point| self.resolve_rally_point(point)));
        let fly_in_started = placement.preferred
            && profile.fly_in
            && self.start_trained_squad_fly_in(
                database,
                player_id,
                squad_id,
                placement,
                rally_destination,
            );
        let trainer_animation_duration = if placement.preferred {
            self.start_trained_birth_animations(building_id, squad_id, &profile, gameplay)
        } else {
            0.0
        };
        if !fly_in_started {
            self.issue_trained_squad_rally(player_id, squad_id, rally_destination);
        }
        let delay = self.trained_birth_delay(
            squad_id,
            placement.obstruction_radius,
            rally.is_some() || birth.plotted_position().is_some() || profile.force_delay,
            trainer_animation_duration,
        );
        if let Some(building) = self.get_building_mut(building_id) {
            building.production.set_trained_squad_birth_time(delay);
            let _removed = building.production.remove_trained_squad(squad_id);
        }
    }

    fn trained_squad_containment_snapshot(
        &self,
        building_id: EntityId,
        squad_id: EntityId,
    ) -> Option<(Vec3, Vec3, Option<EntityId>, Vec<EntityId>)> {
        let building = self.get_building(building_id)?;
        let squad = self.get_squad(squad_id)?;
        if !building.is_alive()
            || !squad.is_alive()
            || squad.base.player_id != building.base.player_id
            || squad.unit_ids.is_empty()
            || squad
                .unit_ids
                .iter()
                .any(|unit_id| !self.get_unit(*unit_id).is_some_and(Entity::is_alive))
        {
            return None;
        }
        Some((
            building.base.position,
            planar_forward(building.base.forward),
            building.squad_id,
            squad.unit_ids.clone(),
        ))
    }

    fn remove_missing_trained_births(&mut self, building_id: EntityId) {
        let live = self
            .squads
            .iter()
            .filter_map(|(squad_id, squad)| {
                (squad.is_alive()
                    && squad
                        .unit_ids
                        .iter()
                        .any(|unit_id| self.get_unit(*unit_id).is_some_and(Entity::is_alive)))
                .then_some(squad_id)
            })
            .collect::<BTreeSet<_>>();
        if let Some(building) = self.get_building_mut(building_id) {
            building
                .production
                .remove_missing_trained_squads(|squad_id| live.contains(&squad_id));
        }
    }

    fn plot_direct_training_rallies(
        &mut self,
        building_id: EntityId,
        producer_id: EntityId,
        player_id: PlayerId,
    ) {
        let Some(rally) = self
            .trained_birth_rally_point(building_id, producer_id, player_id)
            .filter(|rally| rally.target_entity_id().is_none())
        else {
            return;
        };
        let births = self
            .get_building(building_id)
            .map(|building| {
                building
                    .production
                    .trained_squad_births()
                    .filter(|birth| birth.plotted_position().is_none())
                    .filter(|birth| {
                        self.get_squad(birth.squad_id())
                            .is_some_and(|squad| squad.base.player_id == player_id)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if births.is_empty() {
            return;
        }
        let Some(building_position) = self
            .get_building(building_id)
            .map(|building| building.base.position)
        else {
            return;
        };
        let destination = direct_rally_destination(building_position, rally);
        let squad_ids = births
            .iter()
            .map(|birth| birth.squad_id())
            .collect::<Vec<_>>();
        let plots = plot_training_squads(self, &squad_ids, destination);
        if let Some(building) = self.get_building_mut(building_id) {
            for (squad_id, position) in plots {
                building
                    .production
                    .set_trained_squad_plotted_position(squad_id, position);
            }
        }
    }

    fn trained_squad_producer(&self, squad_id: EntityId) -> Option<EntityId> {
        let squad = self.get_squad(squad_id)?;
        squad.trained_by.or_else(|| {
            squad
                .unit_ids
                .iter()
                .find_map(|unit_id| self.get_unit(*unit_id).and_then(|unit| unit.trained_by))
        })
    }

    fn trained_birth_rally_point(
        &self,
        controller_id: EntityId,
        producer_id: EntityId,
        player_id: PlayerId,
    ) -> Option<RallyPoint> {
        self.unit_rally_point(controller_id, player_id)
            .or_else(|| self.unit_rally_point(producer_id, player_id))
            .or_else(|| self.player_rally_point(player_id))
    }

    fn birth_profile(
        &self,
        building_id: EntityId,
        squad_id: EntityId,
        database: &Database,
    ) -> BirthProfile {
        let trainer = self
            .get_building(building_id)
            .and_then(|building| find_object(database, &building.proto_object_name));
        let birth = effective_squad_prototype(self, squad_id, database)
            .and_then(|squad| squad.birth.as_ref());
        let trainer_type = trainer
            .and_then(|prototype| prototype.trainer_type.as_ref())
            .map_or(0, |trainer| trainer.trainer_type);
        let member_animation = birth.and_then(|birth| birth_animation(birth, trainer_type));
        let fly_in = birth
            .and_then(|birth| birth.birth_type.as_deref())
            .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("FlyIn"));
        let force_delay = trainer.is_some_and(|prototype| {
            has_type(prototype, "Hook") && has_type(prototype, "BirthOnTop")
        });
        BirthProfile {
            member_animation,
            trainer_animation: birth.and_then(|birth| birth.trainer_animation.clone()),
            apply_formation: trainer
                .and_then(|prototype| prototype.trainer_type.as_ref())
                .and_then(|trainer| trainer.apply_formation)
                .unwrap_or(false),
            fly_in,
            force_delay,
        }
    }

    fn release_trained_squad(&mut self, squad_id: EntityId, placement: SquadBirthPlacement) {
        self.detach_passenger_refs(squad_id);
        if let Some(squad) = self.get_squad_mut(squad_id) {
            squad.garrison.finish_action();
            squad.remove_all_orders();
            squad.base.position = placement.position;
            squad.base.forward = placement.forward;
            squad.base.velocity = Vec3::ZERO;
            squad.state = SquadState::Idle;
        }
        self.place_squad_members(squad_id, placement.position, placement.forward, true);
    }

    fn start_trained_birth_animations(
        &mut self,
        building_id: EntityId,
        squad_id: EntityId,
        profile: &BirthProfile,
        gameplay: Option<&GameplayCatalog>,
    ) -> f32 {
        if !profile.fly_in
            && let Some(animation) = profile.member_animation.as_deref()
        {
            if !profile.apply_formation
                && let Some((position, forward)) = self
                    .get_squad(squad_id)
                    .map(|squad| (squad.base.position, squad.base.forward))
            {
                self.place_squad_members(squad_id, position, forward, false);
            }
            let members = self
                .get_squad(squad_id)
                .map_or_else(Vec::new, |squad| squad.unit_ids.clone());
            for unit_id in members {
                let prototype = self
                    .get_unit(unit_id)
                    .map(|unit| unit.proto_object_name.clone());
                if let Some(prototype) = prototype {
                    self.play_catalog_animation(unit_id, &prototype, animation, gameplay);
                }
            }
        }
        let Some(animation) = profile.trainer_animation.as_deref() else {
            return 0.0;
        };
        let Some(prototype) = self
            .get_building(building_id)
            .map(|building| building.proto_object_name.clone())
        else {
            return 0.0;
        };
        self.play_catalog_animation(building_id, &prototype, animation, gameplay)
    }

    fn play_catalog_animation(
        &mut self,
        entity_id: EntityId,
        prototype: &str,
        animation: &str,
        gameplay: Option<&GameplayCatalog>,
    ) -> f32 {
        let clip =
            gameplay.and_then(|catalog| catalog.scripted_animation_clip(prototype, animation));
        let duration_ms = if let Some(clip) = clip {
            clip.duration_ms()
        } else {
            0
        };
        let asset = clip.map(|clip| clip.asset_path().to_owned());
        let _played =
            self.play_entity_animation(entity_id, animation.to_owned(), asset, duration_ms);
        duration_ms.to_f32().unwrap_or(f32::MAX) / 1_000.0
    }

    fn issue_trained_squad_rally(
        &mut self,
        player_id: PlayerId,
        squad_id: EntityId,
        destination: Option<Vec3>,
    ) {
        if let Some(destination) = destination {
            let _issued = self.issue_move_order(player_id, squad_id, destination);
        }
    }

    fn trained_birth_delay(
        &self,
        squad_id: EntityId,
        obstruction_radius: f32,
        delay_birth: bool,
        trainer_animation_duration: f32,
    ) -> f32 {
        if !delay_birth {
            return 0.0;
        }
        if trainer_animation_duration > 0.0 {
            return trainer_animation_duration;
        }
        let speed = self
            .get_squad(squad_id)
            .and_then(|squad| squad.unit_ids.first())
            .and_then(|unit_id| self.get_unit(*unit_id))
            .map_or(0.0, |unit| unit.speed);
        if speed > 0.0 && obstruction_radius > 0.0 {
            1.0 + obstruction_radius * 2.0 / speed
        } else {
            0.0
        }
    }
}

fn direct_rally_destination(building_position: Vec3, rally: RallyPoint) -> Vec3 {
    let mut destination = rally.position();
    let direction = destination - building_position;
    if direction.length() > DIRECT_RALLY_CLEARANCE {
        destination -= direction.normalize() * DIRECT_RALLY_CLEARANCE;
    }
    destination
}

fn plot_training_squads(
    world: &World,
    squad_ids: &[EntityId],
    destination: Vec3,
) -> Vec<(EntityId, Vec3)> {
    let average = squad_ids
        .iter()
        .filter_map(|squad_id| world.get_squad(*squad_id))
        .fold(Vec3::ZERO, |sum, squad| sum + squad.base.position)
        / squad_ids.len().max(1).to_f32().unwrap_or(f32::MAX);
    let forward = planar_forward(destination - average);
    let right = Vec3::Y.cross(forward).normalize_or(Vec3::X);
    let dimensions = squad_ids
        .iter()
        .map(|squad_id| squad_dimensions(world, *squad_id))
        .collect::<Vec<_>>();
    let mut plots = Vec::with_capacity(squad_ids.len());
    let mut row_origin = destination;
    for row_start in (0..squad_ids.len()).step_by(2) {
        let second = (row_start + 1 < squad_ids.len()).then_some(row_start + 1);
        let row_depth = second.map_or(dimensions[row_start].1, |index| {
            dimensions[row_start].1.max(dimensions[index].1)
        });
        if row_start > 0 {
            let previous = dimensions[row_start - 2].1.max(
                dimensions
                    .get(row_start - 1)
                    .map_or(0.0, |dimension| dimension.1),
            );
            row_origin -= forward * (previous + PLOTTER_SQUAD_SPACING);
        }
        let first_side = if second.is_some() { -1.0 } else { 0.0 };
        let first_width = dimensions[row_start].0 + PLOTTER_SQUAD_SPACING * 2.0;
        plots.push((
            squad_ids[row_start],
            row_origin - forward * (row_depth * 0.5) + right * (first_side * first_width * 0.5),
        ));
        if let Some(index) = second {
            let width = dimensions[index].0 + PLOTTER_SQUAD_SPACING * 2.0;
            plots.push((
                squad_ids[index],
                row_origin - forward * (row_depth * 0.5) + right * (width * 0.5),
            ));
        }
    }
    plots
}

fn squad_dimensions(world: &World, squad_id: EntityId) -> (f32, f32) {
    let Some(squad) = world.get_squad(squad_id) else {
        return (1.0, 1.0);
    };
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_z = f32::INFINITY;
    let mut max_z = f32::NEG_INFINITY;
    for unit in squad
        .unit_ids
        .iter()
        .filter_map(|unit_id| world.get_unit(*unit_id))
    {
        min_x = min_x.min(unit.formation_offset.x - unit.obstruction_half_extents.x.abs());
        max_x = max_x.max(unit.formation_offset.x + unit.obstruction_half_extents.x.abs());
        min_z = min_z.min(unit.formation_offset.z - unit.obstruction_half_extents.z.abs());
        max_z = max_z.max(unit.formation_offset.z + unit.obstruction_half_extents.z.abs());
    }
    if min_x.is_finite() && min_z.is_finite() {
        ((max_x - min_x).max(1.0), (max_z - min_z).max(1.0))
    } else {
        (1.0, 1.0)
    }
}

fn birth_animation(birth: &Birth, trainer_type: i32) -> Option<String> {
    match trainer_type {
        0 => birth.animation_0.clone(),
        1 => birth.animation_1.clone(),
        2 => birth.animation_2.clone(),
        3 => birth.animation_3.clone(),
        _ => None,
    }
    .filter(|animation| !animation.trim().is_empty())
}

fn find_object<'a>(database: &'a Database, name: &str) -> Option<&'a ProtoObject> {
    database
        .objects
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(name))
}

fn has_type(prototype: &ProtoObject, expected: &str) -> bool {
    prototype
        .flags
        .iter()
        .chain(prototype.object_types.iter())
        .any(|value| value.eq_ignore_ascii_case(expected))
}

fn planar_forward(forward: Vec3) -> Vec3 {
    Vec3::new(forward.x, 0.0, forward.z)
        .try_normalize()
        .unwrap_or(Vec3::Z)
}
