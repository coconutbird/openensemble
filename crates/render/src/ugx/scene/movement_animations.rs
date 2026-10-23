//! Retail movement-animation selection projected from authoritative sim state.

use pipeline::database::hw1::ProtoObject;
use sim::Unit;

const MAX_VELOCITY_MULTIPLIER: f32 = 1.5;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub(super) enum MovementAnimation {
    #[default]
    Idle,
    Walk,
    Jog,
    Run,
    TurnLeft,
    TurnRight,
    WalkIdle,
}

impl MovementAnimation {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Walk => "Walk",
            Self::Jog => "Jog",
            Self::Run => "Run",
            Self::TurnLeft => "TurnLeft",
            Self::TurnRight => "TurnRight",
            Self::WalkIdle => "WalkIdle",
        }
    }

    pub(super) const fn is_motion(self) -> bool {
        matches!(
            self,
            Self::Walk | Self::Jog | Self::Run | Self::TurnLeft | Self::TurnRight
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct MovementAnimationProfile {
    desired_velocity: f32,
    maximum_velocity: f32,
    no_action_override_move: bool,
    turn_in_place: bool,
    ik_transition_to_idle: bool,
}

impl MovementAnimationProfile {
    pub(super) fn from_proto(proto: &ProtoObject) -> Self {
        let desired_velocity = finite_nonnegative(proto.velocity);
        let maximum_velocity = proto
            .max_velocity
            .filter(|velocity| velocity.is_finite() && *velocity >= 0.0)
            .unwrap_or(desired_velocity * MAX_VELOCITY_MULTIPLIER);
        Self {
            desired_velocity,
            maximum_velocity,
            no_action_override_move: has_flag(proto, "NoActionOverrideMove"),
            turn_in_place: has_flag(proto, "TurnInPlace"),
            ik_transition_to_idle: has_flag(proto, "IKTransitionToIdle"),
        }
    }

    pub(super) fn select(self, unit: &Unit, previous: MovementAnimation) -> MovementAnimation {
        if self.turn_in_place
            && let Some(direction) = unit.turn_in_place_direction()
        {
            return match direction {
                sim::TurnInPlaceDirection::Left => MovementAnimation::TurnLeft,
                sim::TurnInPlaceDirection::Right => MovementAnimation::TurnRight,
            };
        }
        if !unit.has_active_move_action() {
            if self.ik_transition_to_idle
                && (previous.is_motion() || previous == MovementAnimation::WalkIdle)
            {
                return MovementAnimation::WalkIdle;
            }
            return MovementAnimation::Idle;
        }

        let (walk_jog_scale, jog_run_scale) = match previous {
            MovementAnimation::Jog => (0.25, 0.75),
            MovementAnimation::Run => (0.25, 0.25),
            _ => (0.75, 0.75),
        };
        let maximum_walk_velocity = self.desired_velocity * walk_jog_scale;
        let minimum_run_velocity =
            self.desired_velocity + (self.maximum_velocity - self.desired_velocity) * jog_run_scale;
        let velocity = unit.base.velocity.length();
        if velocity <= maximum_walk_velocity {
            MovementAnimation::Walk
        } else if velocity >= minimum_run_velocity {
            MovementAnimation::Run
        } else {
            MovementAnimation::Jog
        }
    }

    pub(super) fn lower_body_track(
        self,
        unit: &Unit,
        movement: MovementAnimation,
        action_animation: Option<&str>,
    ) -> Option<MovementAnimation> {
        if movement != MovementAnimation::Idle {
            return Some(movement);
        }
        let action_is_idle_or_death = action_animation.is_none_or(|animation| {
            animation.eq_ignore_ascii_case("Idle") || animation.eq_ignore_ascii_case("Death")
        });
        (self.no_action_override_move && unit.base.is_alive() && !action_is_idle_or_death)
            .then_some(MovementAnimation::Idle)
    }
}

fn finite_nonnegative(value: Option<f32>) -> f32 {
    value
        .filter(|velocity| velocity.is_finite() && *velocity >= 0.0)
        .unwrap_or_default()
}

fn has_flag(proto: &ProtoObject, name: &str) -> bool {
    proto
        .flags
        .iter()
        .any(|flag| flag.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use glam::Vec3;
    use pipeline::database::hw1::{Database, ProtoObject};

    use super::{MovementAnimation, MovementAnimationProfile};

    fn profile() -> MovementAnimationProfile {
        MovementAnimationProfile::from_proto(&ProtoObject {
            velocity: Some(10.0),
            max_velocity: Some(15.0),
            ..ProtoObject::default()
        })
    }

    fn moving_unit(speed: f32) -> sim::Unit {
        let mut world = sim::World::new();
        let id = world.create_unit(0);
        let unit = world.get_unit_mut(id).unwrap();
        unit.move_target = Some(Vec3::Z * 100.0);
        unit.base.velocity = Vec3::Z * speed;
        unit.clone()
    }

    #[test]
    fn thresholds_use_the_current_action_animation_hysteresis() {
        let profile = profile();
        assert_eq!(
            profile.select(&moving_unit(7.5), MovementAnimation::Idle),
            MovementAnimation::Walk
        );
        assert_eq!(
            profile.select(&moving_unit(8.0), MovementAnimation::Idle),
            MovementAnimation::Jog
        );
        assert_eq!(
            profile.select(&moving_unit(13.75), MovementAnimation::Idle),
            MovementAnimation::Run
        );
        assert_eq!(
            profile.select(&moving_unit(3.0), MovementAnimation::Jog),
            MovementAnimation::Jog
        );
        assert_eq!(
            profile.select(&moving_unit(11.25), MovementAnimation::Run),
            MovementAnimation::Run
        );
    }

    #[test]
    fn velocity_without_a_move_action_does_not_select_a_move_clip() {
        let mut unit = moving_unit(15.0);
        unit.move_target = None;
        assert_eq!(
            profile().select(&unit, MovementAnimation::Run),
            MovementAnimation::Idle
        );
    }

    #[test]
    fn squad_ground_move_action_selects_from_authoritative_sim_state() {
        let mut world = sim::World::new();
        let squad_id = world.create_squad_at(1, Vec3::ZERO);
        let unit_id = world.create_unit_at(1, Vec3::ZERO);
        {
            let squad = world.get_squad_mut(squad_id).unwrap();
            squad.speed = 10.0;
            squad.acceleration = 26.0;
            squad.move_to(Vec3::X * 30.0);
        }
        {
            let unit = world.get_unit_mut(unit_id).unwrap();
            unit.speed = 10.0;
            unit.turn_rate_degrees = 540.0;
        }
        assert!(world.attach_unit_to_squad(unit_id, squad_id));

        world.update_entities(0.05);

        let unit = world.get_unit(unit_id).unwrap();
        assert!(unit.has_active_move_action());
        assert_eq!(
            profile().select(unit, MovementAnimation::Idle),
            MovementAnimation::Walk
        );
    }

    #[test]
    fn no_action_override_move_keeps_idle_on_the_lower_track() {
        let profile = MovementAnimationProfile::from_proto(&ProtoObject {
            flags: vec!["NoActionOverrideMove".to_owned()],
            ..ProtoObject::default()
        });
        let unit = moving_unit(0.0);

        assert_eq!(
            profile.lower_body_track(&unit, MovementAnimation::Idle, Some("Attack")),
            Some(MovementAnimation::Idle)
        );
        assert_eq!(
            profile.lower_body_track(&unit, MovementAnimation::Idle, Some("Death")),
            None
        );
        assert_eq!(
            profile.lower_body_track(&unit, MovementAnimation::Walk, Some("Attack")),
            Some(MovementAnimation::Walk)
        );
    }

    #[test]
    fn turn_in_place_flag_projects_the_authoritative_source_direction() {
        let proto = ProtoObject {
            name: "turner".to_owned(),
            dbid: Some(7),
            object_class: Some("Unit".to_owned()),
            flags: vec!["TurnInPlace".to_owned()],
            velocity: Some(10.0),
            turn_rate: Some(90.0),
            ..ProtoObject::default()
        };
        let profile = MovementAnimationProfile::from_proto(&proto);
        let mut database = Database::new();
        database.objects.push(proto);
        let mut world = sim::World::new();
        world.init_players(1);
        let unit_id = sim::spawn_object_at(&mut world, &database, 1, 0, Vec3::ZERO, Vec3::Z)
            .expect("turn-in-place unit");
        assert!(world.get_unit_mut(unit_id).unwrap().move_to(Vec3::X * 10.0));

        world.update_entities(0.05);

        assert_eq!(
            profile.select(world.get_unit(unit_id).unwrap(), MovementAnimation::Idle),
            MovementAnimation::TurnLeft
        );
    }

    #[test]
    fn ik_transition_to_idle_requests_walk_idle_until_motion_resumes() {
        let profile = MovementAnimationProfile::from_proto(&ProtoObject {
            flags: vec!["IKTransitionToIdle".to_owned()],
            velocity: Some(10.0),
            max_velocity: Some(15.0),
            ..ProtoObject::default()
        });
        let idle = sim::Unit::default();

        assert_eq!(
            profile.select(&idle, MovementAnimation::Idle),
            MovementAnimation::Idle
        );
        assert_eq!(
            profile.select(&idle, MovementAnimation::Walk),
            MovementAnimation::WalkIdle
        );
        assert_eq!(
            profile.select(&idle, MovementAnimation::TurnRight),
            MovementAnimation::WalkIdle
        );
        assert_eq!(
            profile.select(&idle, MovementAnimation::WalkIdle),
            MovementAnimation::WalkIdle
        );
        assert_eq!(
            profile.select(&moving_unit(1.0), MovementAnimation::WalkIdle),
            MovementAnimation::Walk
        );
    }

    #[test]
    fn ordinary_units_stop_directly_on_idle() {
        assert_eq!(
            profile().select(&sim::Unit::default(), MovementAnimation::Run),
            MovementAnimation::Idle
        );
    }
}
