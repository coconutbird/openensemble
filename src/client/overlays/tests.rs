use glam::Vec3;
use render::hp_bars::HpBarStyleId;
use sim::{EntityId, World};

use super::*;
use crate::client::catalog::{BarStyle, ClientCatalog, DecalShape, ObjectInfo, SquadInfo};
use crate::client::catalog::{PickPriority, SelectType};
use crate::client::selection::Selection;

const GREEN: [u8; 3] = [69, 110, 24];
const ORANGE: [u8; 3] = [235, 165, 27];

fn catalog() -> ClientCatalog {
    let mut catalog = ClientCatalog::default();
    catalog.set_selection_color(1, GREEN);
    catalog.set_selection_color(2, ORANGE);
    catalog.insert_object(
        "marine",
        ObjectInfo {
            select_type: SelectType::Unit,
            pick_priority: PickPriority::Unit,
            army_goto_type: true,
            pick_radius: 1.0,
            pick_offset: 0.0,
            must_own_to_select: false,
            sub_select_sort: 3000,
            maxima: [100.0, 0.0, 0.0],
            object_types: Vec::new(),
            unit_hp_bar: Some("Unit_Health".to_owned()),
            obstruction_radius_y: 2.0,
        },
    );
    catalog.insert_squad(
        "marines",
        SquadInfo {
            sub_select_sort: 3000,
            decal: DecalShape {
                radius_x: 3.0,
                radius_z: 4.0,
                y_offset: 0.5,
                z_offset: 1.0,
            },
            hp_bar: Some(BarStyle {
                name: "UNSC_Health".to_owned(),
                size: [0.0, 0.0],
                offset: Vec3::new(0.0, 5.0, 0.0),
            }),
            always_show_hp_bar: false,
            always_render_selection_decal: false,
            members: vec![("marine".to_owned(), 2)],
        },
    );
    catalog
}

fn world() -> World {
    let mut world = World::new();
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(1).unwrap().color = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.get_player_mut(2).unwrap().color = 2;
    world.configure_standard_team_relations();
    world.set_fog_of_war_enabled(false);
    world
}

fn squad(world: &mut World, player: u8, proto_squad: &str, positions: &[Vec3]) -> EntityId {
    let squad_id = world.create_squad_at(player, positions[0]);
    let squad = world.get_squad_mut(squad_id).unwrap();
    proto_squad.clone_into(&mut squad.proto_squad_name);
    squad.base.forward = Vec3::X;
    for &position in positions {
        let unit_id = world.create_unit_at(player, position);
        assert!(world.attach_unit_to_squad(unit_id, squad_id));
        let unit = world.get_unit_mut(unit_id).unwrap();
        "marine".clone_into(&mut unit.proto_object_name);
        unit.max_hitpoints = 100.0;
        unit.hitpoints = 100.0;
    }
    squad_id
}

fn style(name: &str) -> Option<HpBarStyleId> {
    match name {
        "UNSC_Health" => Some(HpBarStyleId(0)),
        "Unit_Health" => Some(HpBarStyleId(1)),
        _ => None,
    }
}

fn build(
    world: &World,
    catalog: &ClientCatalog,
    selection: &Selection,
    hovered: Option<EntityId>,
) -> (Vec<SelectionDecal>, Vec<HpBarInstance>) {
    let context = OverlayContext {
        world,
        catalog,
        selection,
        hovered,
        team: 1,
        always_show_bars: false,
    };
    let mut decals = Vec::new();
    let mut bars = Vec::new();
    build_overlays(&context, style, &mut decals, &mut bars);
    (decals, bars)
}

#[test]
fn selected_squads_get_base_decals_and_the_hover_gets_the_bright_one() {
    let mut world = world();
    let mut catalog = catalog();
    catalog.insert_hp_bar_size("UNSC_Health", [8.0, 2.0]);
    let own = squad(
        &mut world,
        1,
        "marines",
        &[Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0)],
    );
    let enemy = squad(&mut world, 2, "marines", &[Vec3::new(10.0, 0.0, 0.0)]);
    let mut selection = Selection::default();
    selection.select_squad(&world, own);
    selection.rebuild_squads(&world, |_| 0);

    let (decals, bars) = build(&world, &catalog, &selection, None);
    assert_eq!(decals.len(), 1);
    let decal = decals[0];
    // The member average (1, 0, 0) plus `ZOffset` 1 along the forward direction (X).
    assert_eq!(decal.center, Vec3::new(2.0, 0.0, 0.0));
    assert!((decal.radius_x - 3.0).abs() < f32::EPSILON);
    assert!((decal.radius_z - 4.0).abs() < f32::EPSILON);
    assert!((decal.y_offset - 0.5).abs() < f32::EPSILON);
    assert!((decal.intensity - BASE_DECAL_INTENSITY).abs() < f32::EPSILON);
    assert!((decal.color[0] - 69.0 / 255.0).abs() < 1e-6);
    assert!((decal.color[3] - 1.0).abs() < f32::EPSILON);
    // Selection alone does not show a bar.
    assert_eq!(bars, [] as [HpBarInstance; 0]);

    // The hovered enemy gets the hover decal in its own colour and its bar.
    let enemy_unit = world.squads.get(enemy).unwrap().unit_ids[0];
    let (decals, bars) = build(&world, &catalog, &selection, Some(enemy_unit));
    assert_eq!(decals.len(), 2);
    let hover = decals
        .iter()
        .find(|decal| (decal.intensity - SELECTION_DECAL_INTENSITY).abs() < f32::EPSILON)
        .unwrap();
    assert!((hover.color[0] - 235.0 / 255.0).abs() < 1e-6);
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].player_color, ORANGE);
}

#[test]
fn recent_damage_shows_the_squad_bar_with_the_protosquad_ratio() {
    let mut world = world();
    let mut catalog = catalog();
    let own = squad(&mut world, 1, "marines", &[Vec3::ZERO]);
    world.game_time_ms = 10_000;
    world.get_squad_mut(own).unwrap().last_damaged_time = 5_000;
    let unit = world.squads.get(own).unwrap().unit_ids[0];
    world.get_unit_mut(unit).unwrap().hitpoints = 50.0;
    let selection = Selection::default();
    let (_, bars) = build(&world, &catalog, &selection, None);
    assert_eq!(bars, [] as [HpBarInstance; 0], "the style size is unknown");

    // The protosquad has no size, so the `hpbars.xml` style size applies.
    let mut database = pipeline::database::hw1::Database::new();
    database.hp_bars = Some(pipeline::database::hw1::HpBarDefinition {
        hp_bars: vec![pipeline::database::hw1::hpbars::HpBar {
            name: "UNSC_Health".to_owned(),
            size_x: 8.0,
            size_y: 2.0,
            ..Default::default()
        }],
        ..Default::default()
    });
    assert_eq!(
        ClientCatalog::new(&database).hp_bar_size("UNSC_Health"),
        Some([8.0, 2.0])
    );
    catalog.insert_hp_bar_size("UNSC_Health", [8.0, 2.0]);
    let (_, bars) = build(&world, &catalog, &selection, None);
    assert_eq!(bars.len(), 1);
    let bar = bars[0];
    assert_eq!(bar.position, Vec3::new(0.0, 5.0, 0.0));
    assert_eq!(bar.size, [8.0, 2.0]);
    assert_eq!(bar.style, HpBarStyleId(0));
    // One of two members lives at half health: 50 / 200.
    assert!((bar.hitpoints - 0.25).abs() < 1e-6);
    assert_eq!(bar.player_color, GREEN);

    // After 6000 ms the bar goes away.
    world.game_time_ms = 11_001;
    let (_, bars) = build(&world, &catalog, &selection, None);
    assert_eq!(bars, [] as [HpBarInstance; 0]);
}

#[test]
fn a_squad_without_a_protosquad_gets_unit_bars() {
    let mut world = world();
    let catalog = catalog();
    let loose = squad(&mut world, 1, "unknown", &[Vec3::ZERO]);
    let unit = world.squads.get(loose).unwrap().unit_ids[0];
    world.get_unit_mut(unit).unwrap().max_hitpoints = 4000.0;
    world.get_unit_mut(unit).unwrap().hitpoints = 1000.0;
    let selection = Selection::default();
    let (decals, bars) = build(&world, &catalog, &selection, Some(unit));
    // No protosquad: no decal size.
    assert_eq!(decals, [] as [SelectionDecal; 0]);
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].position, Vec3::new(0.0, 4.0, 0.0));
    assert_eq!(bars[0].size, [8.0, 0.75]);
    assert!((bars[0].hitpoints - 0.25).abs() < 1e-6);
}

#[test]
fn unit_bar_size_follows_the_max_hit_points() {
    let close = |(width, height): (f32, f32), expected: (f32, f32)| {
        (width - expected.0).abs() < 1e-5 && (height - expected.1).abs() < 1e-5
    };
    assert!(close(unit_bar_size(100.0), (2.0, 0.6)));
    assert!(close(unit_bar_size(0.0), (2.0, 0.6)));
    assert!(close(unit_bar_size(5000.0), (10.0, 0.75)));
    assert!(close(unit_bar_size(50_000.0), (20.0, 0.75)));
    assert!(close(unit_bar_size(1000.0), (2.0, 0.6)));
}
