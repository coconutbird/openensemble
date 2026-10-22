use glam::Vec3;
use pipeline::database::hw1::flashdecals::{CivDecalList, FlashDecalList, FlashIcon};
use pipeline::database::hw1::squads::Selection;
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad, Vector3};

use super::*;

const WARTHOG: &str = "unsc_veh_warthog_01";
const COMMAND: &str = "unsc_bldg_command_03";

fn icon(kind: &str, owner: &str, size: [f32; 3], offset: Option<Vector3>) -> FlashIcon {
    FlashIcon {
        kind: Some(kind.to_owned()),
        owner: Some(owner.to_owned()),
        size_x: Some(size[0]),
        size_y: (size[1] != 0.0).then_some(size[1]),
        size_z: Some(size[2]),
        offset,
        ..FlashIcon::default()
    }
}

/// A database with the shipped Warthog squad, the HQ object (implicit squad) and a UNSC decal
/// list with their shipped icons.
fn database() -> Database {
    let mut database = Database::new();
    database.objects.push(ProtoObject {
        name: COMMAND.to_owned(),
        object_class: Some("Building".to_owned()),
        selected_radius_x: Some(20.0),
        selected_radius_z: Some(20.0),
        ..ProtoObject::default()
    });
    database.squads.push(ProtoSquad {
        name: WARTHOG.to_owned(),
        selection: Some(Selection {
            z_offset: Some(0.5),
            ..Selection::default()
        }),
        ..ProtoSquad::default()
    });
    let offset = Vector3 {
        x: 3.0,
        y: -1.5,
        z: 0.0,
    };
    database.decal_lists.push(CivDecalList {
        civ: "UNSC".to_owned(),
        path: "art\\ui\\flash\\hud\\hud_decal\\hud_decal.xml".to_owned(),
        list: FlashDecalList {
            icons: vec![
                icon("miscicon", WARTHOG, [15.0, 0.0, 15.0], None),
                icon("uniticon", "UNSC_VEH_WARTHOG_01", [6.0, 0.0, 6.0], None),
                icon("clumpdecal", WARTHOG, [1.0, 0.0, 1.0], None),
                icon("uniticon", COMMAND, [37.0, 2.0, 37.0], Some(offset)),
            ],
            ..FlashDecalList::default()
        },
    });
    database
}

#[test]
fn a_warthog_squad_gets_the_six_by_six_icon_of_its_civ_list() {
    let catalog = ClientCatalog::new(&database(), Some("UNSC"));

    // The first bound icon wins; the owner compare ignores case; `miscicon` does not bind.
    let decal = catalog.squad(WARTHOG).expect("Warthog").decal;
    assert_eq!(
        decal,
        DecalShape {
            radius_x: 6.0,
            radius_z: 6.0,
            y_offset: 0.0,
            offset: Vec3::ZERO,
            z_offset: 0.5,
        }
    );
}

#[test]
fn the_hq_implicit_squad_gets_the_thirty_seven_icon_and_its_offset() {
    let catalog = ClientCatalog::new(&database(), Some("unsc"));

    let decal = catalog.squad(COMMAND).expect("HQ").decal;
    assert_eq!(
        decal,
        DecalShape {
            radius_x: 37.0,
            radius_z: 37.0,
            y_offset: 2.0,
            offset: Vec3::new(3.0, -1.5, 0.0),
            z_offset: 0.0,
        }
    );
}

#[test]
fn without_a_list_of_the_local_civ_the_selection_values_apply() {
    for civ in [None, Some("Covenant")] {
        let catalog = ClientCatalog::new(&database(), civ);

        let warthog = catalog.squad(WARTHOG).expect("Warthog").decal;
        assert_eq!(
            warthog,
            DecalShape {
                z_offset: 0.5,
                ..DecalShape::default()
            }
        );
        let command = catalog.squad(COMMAND).expect("HQ").decal;
        assert_eq!(
            command,
            DecalShape {
                radius_x: 20.0,
                radius_z: 20.0,
                ..DecalShape::default()
            }
        );
    }
}

#[test]
fn icon_sizes_are_half_floats() {
    let mut database = database();
    database.decal_lists[0].list.icons[1].size_x = Some(15.8);

    let catalog = ClientCatalog::new(&database, Some("UNSC"));

    // `BFlashMovieIcon_load` stores 15.8 as the half float 15.796875.
    let decal = catalog.squad(WARTHOG).expect("Warthog").decal;
    assert_eq!(decal.radius_x.to_bits(), 15.796_875_f32.to_bits());
}

/// The shipped decal lists give the Warthog 6 x 6 and the HQ 37 x 37 with its offset, also in
/// the Covenant list.
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_decal_lists_size_the_warthog_and_the_hq() {
    let mut source = pipeline::hw1::loader::load_game_dir(&test_support::game_dir());
    let database = Database::load(&mut source).expect("the installed database should load");

    for civ in ["UNSC", "Covenant"] {
        let catalog = ClientCatalog::new(&database, Some(civ));
        let warthog = catalog.squad(WARTHOG).expect("Warthog squad").decal;
        assert_eq!((warthog.radius_x, warthog.radius_z), (6.0, 6.0), "{civ}");
        assert_eq!(warthog.offset, Vec3::ZERO, "{civ}");
        let command = catalog.squad(COMMAND).expect("HQ implicit squad").decal;
        assert_eq!(
            (command.radius_x, command.radius_z, command.y_offset),
            (37.0, 37.0, 2.0),
            "{civ}"
        );
        assert_eq!(command.offset, Vec3::new(3.0, -1.5, 0.0), "{civ}");
    }
}
