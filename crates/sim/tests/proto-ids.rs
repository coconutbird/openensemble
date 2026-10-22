//! Checks the engine proto ID convention against the installed database.
//!
//! The engine IDs are load-order indices (`BDatabase::setupProtoObjects`,
//! `BDatabase::setupProtoSquads`); `dbid` is only a save-game key.

use sim::{object_prototype_id, squad_prototype_id, squad_prototype_name};

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test proto-ids -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn installed_proto_ids_round_trip_and_implicit_squads_resolve() {
    let loaded = test_support::installed_scenario("blood_gulch");
    let database = &loaded.content.database;

    for (index, object) in database.objects.iter().enumerate() {
        let id = object_prototype_id(database, &object.name).unwrap();
        assert_eq!(usize::try_from(id).unwrap(), index, "{}", object.name);
    }
    // Shares dbid 3436 with hook_bldg_PelicanMediumCoverCinematic_01.
    let cover = object_prototype_id(database, "hook_bldg_PelicanMediumCover_01").unwrap();
    assert_eq!(
        database.objects[usize::try_from(cover).unwrap()].name,
        "hook_bldg_PelicanMediumCover_01"
    );

    for (index, squad) in database.squads.iter().enumerate() {
        assert_eq!(
            squad_prototype_id(database, &squad.name),
            i32::try_from(index).ok(),
            "{}",
            squad.name
        );
    }
    // The merged squads follow the records; the implicit squads follow them.
    let first_after_records = i32::try_from(database.squads.len()).unwrap();
    let merged = squad_prototype_name(database, first_after_records)
        .expect("retail squads.xml declares five merged squads");
    assert!(merged.starts_with("merged_unsc_inf_marine_01_"), "{merged}");
    assert_eq!(
        squad_prototype_id(database, &merged),
        Some(first_after_records)
    );
    // The command center has no squad record: its name resolves to its
    // implicit squad, and the ID resolves back to the name.
    let command = squad_prototype_id(database, "unsc_bldg_command_01").unwrap();
    assert!(command >= first_after_records + 5);
    assert_eq!(
        squad_prototype_name(database, command).as_deref(),
        Some("unsc_bldg_command_01")
    );
}
