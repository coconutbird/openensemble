//! Installed-asset coverage for simulation-owned attack and recoil bone frames.

use sim::{AttackAnimation, AttackAnimationEventKind, GameplayCatalog};

static LOGGER: AnchorLogger = AnchorLogger;

struct AnchorLogger;

impl log::Log for AnchorLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Debug
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata())
            && record
                .args()
                .to_string()
                .contains("Could not resolve posed event anchors")
        {
            eprintln!("{}", record.args());
        }
    }

    fn flush(&self) {}
}

#[test]
#[ignore = "requires OPENENSEMBLE_GAME_DIR pointing at Halo Wars DE"]
fn shipped_attack_and_physics_events_resolve_headless_pose_anchors() {
    let _logger = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Debug);
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point at Halo Wars DE");
    let loaded = sim::load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("scenario and layered gameplay catalog");
    let (attack_count, fallback_attacks, impulse_count, single_bone_count) =
        anchor_counts(&loaded.simulation.gameplay);

    println!("posed attack anchors {attack_count}");
    println!("sim-center attack fallbacks {fallback_attacks}");
    println!("posed physics-impulse anchors {impulse_count}");
    println!("single-bone descendant anchors {single_bone_count}");
    assert!(
        attack_count > 0,
        "installed attack profiles have no posed Attack tags"
    );
    assert!(
        impulse_count > 0,
        "installed attack profiles have no posed PhysicsImpulse tags"
    );
    assert!(
        single_bone_count > 0,
        "installed attack profiles have no single-bone descendant anchors"
    );
}

fn anchor_counts(gameplay: &GameplayCatalog) -> (usize, usize, usize, usize) {
    let mut attacks = 0;
    let mut fallback_attacks = 0;
    let mut impulses = 0;
    let mut single_bones = 0;
    for (object, animation) in gameplay.objects().flat_map(|object| {
        object.attack_profiles().flat_map(move |profile| {
            profile
                .animations
                .iter()
                .chain(
                    profile
                        .charged_animation
                        .iter()
                        .flat_map(|charged| &charged.animations),
                )
                .map(move |animation| (object.proto_object_name(), animation))
        })
    }) {
        let counts = animation_anchor_counts(object, animation);
        attacks += counts.0;
        fallback_attacks += counts.1;
        impulses += counts.2;
        single_bones += counts.3;
    }
    (attacks, fallback_attacks, impulses, single_bones)
}

fn animation_anchor_counts(
    object: &str,
    animation: &AttackAnimation,
) -> (usize, usize, usize, usize) {
    let mut attacks = 0;
    let mut fallback_attacks = 0;
    let mut impulses = 0;
    let mut single_bones = 0;
    for event in &animation.events {
        let (kind, anchor) = match event.kind {
            AttackAnimationEventKind::Attack { .. } => {
                if event.anchor.is_some() {
                    attacks += 1;
                } else {
                    fallback_attacks += 1;
                }
                ("Attack", event.anchor.as_ref())
            }
            AttackAnimationEventKind::PhysicsImpulse(_) => {
                impulses += 1;
                (
                    "PhysicsImpulse",
                    Some(event.anchor.as_ref().unwrap_or_else(|| {
                        panic!(
                            "{object} {} PhysicsImpulse at {} has no fallback frame",
                            animation.asset_path, event.position
                        )
                    })),
                )
            }
        };
        let Some(anchor) = anchor else {
            continue;
        };
        single_bones += anchor.single_bone_poses.len();
        let matrix = anchor.unit_transform(|_| None);
        assert!(
            matrix.to_cols_array().iter().all(|value| value.is_finite()),
            "{object} {} {kind} produced a non-finite pose anchor",
            animation.asset_path
        );
    }
    (attacks, fallback_attacks, impulses, single_bones)
}
