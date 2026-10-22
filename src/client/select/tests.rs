use glam::{Vec2, Vec3};
use openensemble::session::world_camera;
use render::picking::project_point;
use render::terrain::Camera;
use render::world_view::WorldCamera;
use sim::{EntityId, World};

use super::*;
use crate::client::camera::RtsCamera;
use crate::client::catalog::{DecalShape, ObjectInfo, PickPriority, SelectType, SquadInfo};
use crate::client::hover::{Hover, HoverType};
use crate::client::selection::Selection;

const VIEWPORT: (u32, u32) = (800, 600);

fn object(select_type: SelectType, army: bool, sort: i32) -> ObjectInfo {
    ObjectInfo {
        select_type,
        pick_priority: PickPriority::Unit,
        army_goto_type: army,
        pick_radius: 1.0,
        pick_offset: 0.0,
        must_own_to_select: false,
        sub_select_sort: sort,
        maxima: [100.0, 0.0, 0.0],
        object_types: Vec::new(),
        unit_hp_bar: None,
        obstruction_radius_y: 1.0,
    }
}

fn catalog() -> ClientCatalog {
    let mut catalog = ClientCatalog::default();
    catalog.insert_object("marine", object(SelectType::Unit, true, 3000));
    catalog.insert_object("warthog", object(SelectType::Unit, true, 500));
    catalog.insert_object("civilian", object(SelectType::Unit, false, 10));
    catalog.insert_object("hero", object(SelectType::SingleUnit, true, 1000));
    let mut barracks = object(SelectType::Command, false, 20);
    barracks.pick_priority = PickPriority::Building;
    barracks.object_types = vec!["Building".to_owned()];
    catalog.insert_object("barracks", barracks);
    catalog
}

fn world() -> World {
    let mut world = World::new();
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    world
}

/// Makes a squad of `count` units of `proto` for `player` at `position`.
fn squad(world: &mut World, player: u8, proto: &str, position: Vec3, count: usize) -> EntityId {
    let squad_id = world.create_squad_at(player, position);
    proto.clone_into(&mut world.get_squad_mut(squad_id).unwrap().proto_squad_name);
    for _ in 0..count {
        let unit_id = world.create_unit_at(player, position);
        assert!(world.attach_unit_to_squad(unit_id, squad_id));
        proto.clone_into(&mut world.get_unit_mut(unit_id).unwrap().proto_object_name);
    }
    squad_id
}

fn first_unit(world: &World, squad: EntityId) -> EntityId {
    world.squads.get(squad).unwrap().unit_ids[0]
}

fn camera() -> WorldCamera {
    let mut camera = Camera::default();
    RtsCamera::new(Vec3::ZERO).apply_to(&mut camera);
    world_camera(&camera, VIEWPORT)
}

fn hover_on(unit: EntityId, point: Vec3) -> Hover {
    Hover {
        point: Some(point),
        entity: Some(unit),
        kind: HoverType::Selectable,
        candidates: Vec::new(),
    }
}

struct Fixture {
    world: World,
    catalog: ClientCatalog,
    camera: WorldCamera,
}

impl Fixture {
    fn new() -> Self {
        Self {
            world: world(),
            catalog: catalog(),
            camera: camera(),
        }
    }

    fn context(&self) -> SelectContext<'_> {
        SelectContext {
            world: &self.world,
            catalog: &self.catalog,
            player: 1,
            camera: &self.camera,
        }
    }

    fn screen(&self, position: Vec3) -> Vec2 {
        project_point(&self.camera, position).expect("the point is in front of the camera")
    }
}

#[test]
fn drag_box_selects_own_army_units_and_whole_squads() {
    let mut fixture = Fixture::new();
    let marines = squad(&mut fixture.world, 1, "marine", Vec3::ZERO, 3);
    let enemy = squad(&mut fixture.world, 2, "marine", Vec3::new(1.0, 0.0, 0.0), 1);
    let civilian = squad(
        &mut fixture.world,
        1,
        "civilian",
        Vec3::new(-1.0, 0.0, 0.0),
        1,
    );
    let barracks = squad(
        &mut fixture.world,
        1,
        "barracks",
        Vec3::new(0.0, 0.0, 1.0),
        1,
    );
    let far = squad(
        &mut fixture.world,
        1,
        "marine",
        Vec3::new(60.0, 0.0, 60.0),
        1,
    );
    // Move one marine out of the box: the squad still comes in whole.
    let outside_marine = fixture.world.squads.get(marines).unwrap().unit_ids[2];
    fixture
        .world
        .get_unit_mut(outside_marine)
        .unwrap()
        .base
        .position = Vec3::new(40.0, 0.0, -40.0);

    let center = fixture.screen(Vec3::ZERO);
    let start = center - Vec2::splat(60.0);
    let end = center + Vec2::splat(60.0);
    let mut selection = Selection::default();
    let context = fixture.context();
    assert!(drag_select(&context, &mut selection, start, end, &[]));
    assert_eq!(selection.squads(), [marines]);
    assert_eq!(selection.units().len(), 3);
    for other in [enemy, civilian, barracks, far] {
        assert!(!selection.contains_squad(other));
    }

    // A box of less than 2 pixels changes nothing.
    assert!(!drag_select(
        &context,
        &mut selection,
        center,
        center + Vec2::new(1.0, 30.0),
        &[]
    ));
    assert_eq!(selection.squads(), [marines]);

    // A Shift drag keeps the saved units of the drag start.
    let saved = vec![first_unit(&fixture.world, far)];
    assert!(drag_select(&context, &mut selection, start, end, &saved));
    assert_eq!(selection.squads().len(), 2);
    assert!(selection.contains_squad(far));
}

#[test]
fn click_replaces_and_buildings_open_their_menu() {
    let mut fixture = Fixture::new();
    let marines = squad(&mut fixture.world, 1, "marine", Vec3::ZERO, 2);
    let warthog = squad(&mut fixture.world, 1, "warthog", Vec3::X, 1);
    let barracks = squad(&mut fixture.world, 1, "barracks", Vec3::Z, 1);
    let enemy_barracks = squad(&mut fixture.world, 2, "barracks", Vec3::NEG_Z, 1);
    let context_world = &fixture;
    let context = context_world.context();
    let mut selection = Selection::default();
    let mut cycle = ClickCycle::default();

    let marine = first_unit(&fixture.world, marines);
    select_at_cursor(
        &context,
        &mut selection,
        &hover_on(marine, Vec3::ZERO),
        true,
        &mut cycle,
    );
    assert_eq!(selection.squads(), [marines]);
    assert_eq!(selection.units().len(), 2);

    // A replacing click on another unit replaces the selection.
    let car = first_unit(&fixture.world, warthog);
    select_at_cursor(
        &context,
        &mut selection,
        &hover_on(car, Vec3::X),
        true,
        &mut cycle,
    );
    assert_eq!(selection.squads(), [warthog]);

    // A click on the ground keeps the selection.
    let ground = Hover {
        point: Some(Vec3::new(5.0, 0.0, 5.0)),
        ..Hover::default()
    };
    select_at_cursor(&context, &mut selection, &ground, true, &mut cycle);
    assert_eq!(selection.squads(), [warthog]);

    // An own building replaces the selection and opens its menu.
    let building = first_unit(&fixture.world, barracks);
    select_at_cursor(
        &context,
        &mut selection,
        &hover_on(building, Vec3::Z),
        true,
        &mut cycle,
    );
    assert_eq!(selection.squads(), [barracks]);
    assert_eq!(selection.menu_building(), Some(building));

    // An enemy building is selected without a menu.
    let enemy = first_unit(&fixture.world, enemy_barracks);
    select_at_cursor(
        &context,
        &mut selection,
        &hover_on(enemy, Vec3::NEG_Z),
        true,
        &mut cycle,
    );
    assert_eq!(selection.squads(), [enemy_barracks]);
    assert_eq!(selection.menu_building(), None);
}

#[test]
fn adding_click_needs_the_same_owner_and_select_type() {
    let mut fixture = Fixture::new();
    let marines = squad(&mut fixture.world, 1, "marine", Vec3::ZERO, 1);
    let enemy = squad(&mut fixture.world, 2, "marine", Vec3::X, 1);
    let hero = squad(&mut fixture.world, 1, "hero", Vec3::Z, 1);
    let warthog = squad(&mut fixture.world, 1, "warthog", Vec3::NEG_X, 1);
    let context = fixture.context();
    let mut selection = Selection::default();
    let mut cycle = ClickCycle::default();
    let marine = first_unit(&fixture.world, marines);
    select_at_cursor(
        &context,
        &mut selection,
        &hover_on(marine, Vec3::ZERO),
        true,
        &mut cycle,
    );

    // Not replacing: an enemy unit has another owner.
    let enemy_unit = first_unit(&fixture.world, enemy);
    select_at_cursor(
        &context,
        &mut selection,
        &hover_on(enemy_unit, Vec3::X),
        false,
        &mut cycle,
    );
    assert_eq!(selection.squads(), [marines]);

    // Not replacing: a unit of the same select type is added.
    let car = first_unit(&fixture.world, warthog);
    select_at_cursor(
        &context,
        &mut selection,
        &hover_on(car, Vec3::NEG_X),
        false,
        &mut cycle,
    );
    assert_eq!(selection.squads().len(), 2);
    assert!(selection.contains_squad(warthog));

    // A SingleUnit object always replaces the selection.
    let hero_unit = first_unit(&fixture.world, hero);
    select_at_cursor(
        &context,
        &mut selection,
        &hover_on(hero_unit, Vec3::Z),
        false,
        &mut cycle,
    );
    assert_eq!(selection.squads(), [hero]);

    // A replacing click may select an enemy unit.
    select_at_cursor(
        &context,
        &mut selection,
        &hover_on(enemy_unit, Vec3::X),
        true,
        &mut cycle,
    );
    assert_eq!(selection.squads(), [enemy]);
}

#[test]
fn shift_click_toggles_own_squads_but_not_buildings() {
    let mut fixture = Fixture::new();
    let marines = squad(&mut fixture.world, 1, "marine", Vec3::ZERO, 2);
    let warthog = squad(&mut fixture.world, 1, "warthog", Vec3::X, 1);
    let barracks = squad(&mut fixture.world, 1, "barracks", Vec3::Z, 1);
    let enemy = squad(&mut fixture.world, 2, "marine", Vec3::NEG_X, 1);
    let context = fixture.context();
    let mut selection = Selection::default();

    let hover = |squad| hover_on(first_unit(&fixture.world, squad), Vec3::ZERO);
    toggle_select_hovered(&context, &mut selection, &hover(marines));
    toggle_select_hovered(&context, &mut selection, &hover(warthog));
    assert_eq!(selection.squads().len(), 2);
    toggle_select_hovered(&context, &mut selection, &hover(marines));
    assert_eq!(selection.squads(), [warthog]);
    toggle_select_hovered(&context, &mut selection, &hover(barracks));
    toggle_select_hovered(&context, &mut selection, &hover(enemy));
    assert_eq!(selection.squads(), [warthog]);
}

#[test]
fn double_click_selects_the_same_kind_in_view() {
    let mut fixture = Fixture::new();
    let near = squad(&mut fixture.world, 1, "marine", Vec3::ZERO, 1);
    let second = squad(&mut fixture.world, 1, "marine", Vec3::new(3.0, 0.0, 0.0), 1);
    let behind = squad(
        &mut fixture.world,
        1,
        "marine",
        Vec3::new(0.0, 0.0, 900.0),
        1,
    );
    let other_kind = squad(&mut fixture.world, 1, "warthog", Vec3::X, 1);
    let enemy = squad(&mut fixture.world, 2, "marine", Vec3::NEG_X, 1);
    // Put `behind` behind the eye, out of the frustum.
    let eye = fixture.camera.position;
    let behind_unit = first_unit(&fixture.world, behind);
    fixture
        .world
        .get_unit_mut(behind_unit)
        .unwrap()
        .base
        .position = eye + (eye - Vec3::ZERO).normalize() * 50.0;
    let context = fixture.context();
    let world = &fixture.world;

    let mut selection = Selection::default();
    let hover = hover_on(first_unit(world, near), Vec3::ZERO);
    double_click_select(&context, &mut selection, &hover, false, false);
    assert!(selection.contains_squad(near));
    assert!(selection.contains_squad(second));
    for squad in [behind, other_kind, enemy] {
        assert!(!selection.contains_squad(squad));
    }

    // With add, the squads of the earlier selection stay.
    let mut selection = Selection::default();
    let car = first_unit(world, other_kind);
    toggle_select_hovered(&context, &mut selection, &hover_on(car, Vec3::X));
    double_click_select(&context, &mut selection, &hover, true, true);
    assert!(selection.contains_squad(other_kind));
    assert!(selection.contains_squad(near));
}

#[test]
fn screen_select_keeps_the_view_and_global_select_takes_all() {
    let mut fixture = Fixture::new();
    let near = squad(&mut fixture.world, 1, "marine", Vec3::ZERO, 1);
    let far = squad(
        &mut fixture.world,
        1,
        "warthog",
        Vec3::new(900.0, 0.0, 900.0),
        1,
    );
    let barracks = squad(&mut fixture.world, 1, "barracks", Vec3::Z, 1);
    for (name, sort) in [("marine", 3000), ("warthog", 500)] {
        fixture.catalog.insert_squad(
            name,
            SquadInfo {
                sub_select_sort: sort,
                decal: DecalShape::default(),
                hp_bar: None,
                always_show_hp_bar: false,
                always_render_selection_decal: false,
                members: vec![(name.to_owned(), 1)],
            },
        );
    }
    let context = fixture.context();
    let mut selection = Selection::default();
    select_all(&context, &mut selection, false);
    assert_eq!(selection.squads(), [near]);
    select_all(&context, &mut selection, true);
    // The protosquad `SubSelectSort` puts the warthog (500) before the marine (3000).
    assert_eq!(selection.squads(), [far, near]);
    assert!(!selection.contains_squad(barracks));
}

#[test]
fn frustum_test_rejects_points_behind_the_camera() {
    let camera = camera();
    assert!(sphere_in_frustum(camera.view_projection, Vec3::ZERO, 0.0));
    let behind = camera.position * 2.0;
    assert!(!sphere_in_frustum(camera.view_projection, behind, 1.0));
}
