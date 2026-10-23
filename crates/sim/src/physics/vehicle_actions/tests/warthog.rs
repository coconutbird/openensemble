//! The warthog and ghost actions, and the determinism of a driven scene.

use glam::Vec3;
use pipeline::database::hw1::physics::PhysicsVehicleType;

use super::{Scene, TICK, moving_to, squad_at};
use crate::physics::vehicle_actions::input::VehicleActionInput;

/// The body height above flat terrain where the four warthog springs carry
/// `(1 + 0.806) · 21.7`: `50 · (1.3 − len) = 1.806 · 21.7 / 4`, hardpoints 0.05 below.
fn warthog_rest_height() -> f32 {
    let length = 1.3 - (1.0 + 0.806_451_7) * 21.7 / 4.0 / 50.0;
    length + 1.0 + 0.05
}

/// A squad that moves toward `goal` at `speed` and has a move action until it arrives.
struct SquadMover {
    position: Vec3,
    goal: Vec3,
    speed: f32,
}

impl SquadMover {
    fn advance(&mut self) {
        let offset = self.goal - self.position;
        let distance = offset.length();
        let step = self.speed * TICK;
        self.position = if distance <= step {
            self.goal
        } else {
            self.position + offset * (step / distance)
        };
    }

    fn input(&self, unit: crate::physics::vehicle_actions::input::UnitInput) -> VehicleActionInput {
        let arrived = (self.goal - self.position).length() <= 0.0;
        let move_action = (!arrived).then(|| moving_to(self.goal, self.speed));
        VehicleActionInput {
            unit,
            squad: Some(squad_at(self.position, Vec3::Z, move_action)),
            air: None,
        }
    }
}

fn warthog(position: Vec3, height: impl Fn(f32, f32) -> f32) -> Scene {
    Scene::new(PhysicsVehicleType::Warthog, None, position, 0.0, height)
}

#[test]
fn the_warthog_suspension_holds_the_chassis_at_the_rest_height() {
    let start = Vec3::new(60.0, 10.0, 60.0);
    let mut scene = warthog(start, |_, _| 10.0);
    for _ in 0..80 {
        scene.tick(|unit| VehicleActionInput {
            unit,
            squad: Some(squad_at(start, Vec3::Z, None)),
            air: None,
        });
    }
    let height = scene.position().y - 10.0;
    assert!(
        (height - warthog_rest_height()).abs() < 0.03,
        "height {height}, rest {}",
        warthog_rest_height()
    );
    assert!(scene.rotation().y_axis.y > 0.9999);
    assert!(scene.velocity().length() < 0.05, "{}", scene.velocity());
    assert!(!scene.action.in_air());
}

#[test]
fn the_warthog_drives_to_its_squad_and_stops_at_the_goal() {
    let start = Vec3::new(40.0, 10.0, 20.0);
    let mut scene = warthog(start, |_, _| 10.0);
    let goal = Vec3::new(40.0, 10.0, 80.0);
    let mut squad = SquadMover {
        position: start,
        goal,
        speed: 15.0,
    };
    let mut top_speed = 0.0_f32;
    for _ in 0..400 {
        squad.advance();
        scene.tick(|unit| squad.input(unit));
        top_speed = top_speed.max(scene.velocity().length());
    }
    let unit = scene.unit().position;
    let gap = Vec3::new(unit.x - goal.x, 0.0, unit.z - goal.z).length();
    assert!(
        gap < 1.5,
        "the warthog stopped {gap} from the goal at {unit}"
    );
    assert!(scene.velocity().length() < 0.5, "{}", scene.velocity());
    assert!(top_speed > 12.0, "top speed {top_speed}");
    assert!(scene.flags.allow);
}

#[test]
fn the_ghost_strafes_toward_a_squad_at_its_side() {
    let start = Vec3::new(60.0, 10.0, 60.0);
    let mut scene = Scene::new(PhysicsVehicleType::Ghost, None, start, 0.0, |_, _| 10.0);
    let side = Vec3::new(start.x + 10.0, 10.0, start.z);
    let input = VehicleActionInput {
        unit: scene.unit(),
        squad: Some(squad_at(
            side,
            Vec3::Z,
            Some(moving_to(side + Vec3::X * 50.0, 15.0)),
        )),
        air: None,
    };
    let dt = 0.0125;
    scene
        .action
        .apply(scene.physics.world_mut(), &input, &mut scene.flags, dt);
    // The squad is 10 to the right: `rightAcc = 2.5 · (1 · maxVel − 0)` with maxVel 20, and the
    // forward acceleration is 0 because the squad is not ahead.
    let velocity = scene.velocity();
    assert!((velocity.x - dt * 2.5 * 20.0).abs() < 1e-3, "{velocity}");
    assert!(velocity.z.abs() < 1e-3, "{velocity}");
}

#[test]
fn a_driven_scene_gives_the_same_bits_twice() {
    let run = || {
        let hills =
            |x: f32, z: f32| 10.0 + 2.0 * crate::math::sin(x * 0.1) * crate::math::cos(z * 0.07);
        let start = Vec3::new(30.0, hills(30.0, 20.0), 20.0);
        let mut scene = warthog(start, hills);
        let mut squad = SquadMover {
            position: start,
            goal: Vec3::new(70.0, 10.0, 90.0),
            speed: 15.0,
        };
        let mut bits = Vec::new();
        for _ in 0..200 {
            squad.advance();
            scene.tick(|unit| squad.input(unit));
            bits.extend(scene.state_bits());
        }
        bits
    };
    assert_eq!(run(), run());
}
