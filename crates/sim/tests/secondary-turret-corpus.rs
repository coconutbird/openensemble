//! Installed-data audit for retail's independent turret attack executors.

use sim::load_scenario_from_game_dir;

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_secondary_and_slave_turret_actions_remain_discoverable() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("scenario archive and layered gameplay should load");
    let mut secondary_count = 0;
    let mut slave_count = 0;

    for object in loaded.simulation.gameplay.objects() {
        let tactics = object.tactics();
        for action in &tactics.actions {
            let Some(kind) = action.action_type.as_deref() else {
                continue;
            };
            if !kind.eq_ignore_ascii_case("SecondaryTurretAttack")
                && !kind.eq_ignore_ascii_case("SlaveTurretAttack")
            {
                continue;
            }
            if kind.eq_ignore_ascii_case("SecondaryTurretAttack") {
                secondary_count += 1;
            } else {
                slave_count += 1;
            }
            let weapon = action.weapon.as_deref().and_then(|name| {
                tactics
                    .weapons
                    .iter()
                    .find(|weapon| weapon.name.eq_ignore_ascii_case(name))
            });
            let persistent = tactics.tactic.as_ref().is_some_and(|rules| {
                rules
                    .persistent_actions
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(&action.name))
            });
            println!(
                "{}|{}|{}|weapon={}|hardpoint={}|alternate={}|slave={}|anim={}|persistent={persistent}|lockdown={}|dont-loop={}|wait-disabled={}",
                object.proto_object_name(),
                action.name,
                kind,
                weapon.map_or("-", |weapon| weapon.name.as_str()),
                weapon
                    .and_then(|weapon| weapon.hardpoint.as_deref())
                    .unwrap_or("-"),
                weapon
                    .and_then(|weapon| weapon.alternate_hardpoint.as_deref())
                    .unwrap_or("-"),
                action.slave_attack_action.as_deref().unwrap_or("-"),
                action
                    .anim
                    .as_ref()
                    .map_or("-", |animation| animation.name.as_str()),
                action.requires_lockdown.unwrap_or(false),
                action.dont_loop_attack_anim.unwrap_or(false),
                action.disable_attack_wait_timer.unwrap_or(false),
            );
        }
    }

    println!("secondary={secondary_count} slave={slave_count}");
    assert!(
        secondary_count > 0,
        "installed data has no secondary turrets"
    );
    assert!(slave_count > 0, "installed data has no slave turrets");
}
