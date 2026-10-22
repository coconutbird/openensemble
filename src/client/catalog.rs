//! The proto data that the client uses for selection, hover and overlays.
//!
//! [`ClientCatalog`] reads the values once from the database. The keys are the exact proto
//! names, which are also the names that the simulation stores on its units and squads.

use std::collections::HashMap;

use glam::Vec3;
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};

/// The retail default of a missing `SubSelectSort` (`0x7FFFFFFF`).
pub(crate) const DEFAULT_SUB_SELECT_SORT: i32 = i32::MAX;

/// Proto `SelectType` (`BDatabase_getSelectTypeFromName`, `0x1401F7A80`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SelectType {
    /// 0: the object cannot be selected or hovered.
    #[default]
    None,
    /// 1: a unit that the drag box and the double click select.
    Unit,
    /// 2: a building with a command menu.
    Command,
    /// 3: a target that only the hover of a selection uses.
    Target,
    /// 4: a unit that is always selected alone.
    SingleUnit,
    /// 5: a unit that is selected only with units of the same proto.
    SingleType,
}

impl SelectType {
    fn from_name(name: Option<&str>) -> Self {
        match name.map(str::trim) {
            Some(name) if name.eq_ignore_ascii_case("Unit") => Self::Unit,
            Some(name) if name.eq_ignore_ascii_case("Command") => Self::Command,
            Some(name) if name.eq_ignore_ascii_case("Target") => Self::Target,
            Some(name) if name.eq_ignore_ascii_case("SingleUnit") => Self::SingleUnit,
            Some(name) if name.eq_ignore_ascii_case("SingleType") => Self::SingleType,
            _ => Self::None,
        }
    }
}

/// Proto `PickPriority` (`BDatabase_getPickPriorityFromName`, `0x1401F79F0`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PickPriority {
    /// 0: no priority.
    #[default]
    None,
    /// 1: a building.
    Building,
    /// 2: a resource.
    Resource,
    /// 3: a unit.
    Unit,
    /// 4: a rally point.
    Rally,
}

impl PickPriority {
    fn from_name(name: Option<&str>) -> Self {
        match name.map(str::trim) {
            Some(name) if name.eq_ignore_ascii_case("Building") => Self::Building,
            Some(name) if name.eq_ignore_ascii_case("Resource") => Self::Resource,
            Some(name) if name.eq_ignore_ascii_case("Unit") => Self::Unit,
            Some(name) if name.eq_ignore_ascii_case("Rally") => Self::Rally,
            _ => Self::None,
        }
    }
}

/// The proto `GotoType` values that the drag box and the screen select accept: Infantry (3),
/// Vehicle (4), Air (5) and Hero (11) (`BUser_dragSelect`, `0x140585BC0`).
fn goto_type_is_army(name: Option<&str>) -> bool {
    name.map(str::trim).is_some_and(|name| {
        ["Infantry", "Vehicle", "Air", "Hero"]
            .iter()
            .any(|army| name.eq_ignore_ascii_case(army))
    })
}

/// An HP bar of a proto: the `hpbars.xml` style name, the size and the offset.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BarStyle {
    /// The `HPBar` style name, for example `UNSC_Health`.
    pub(crate) name: String,
    /// The proto `sizeX` and `sizeY`. A value of 1e-6 or less selects the style size.
    pub(crate) size: [f32; 2],
    /// The offset from the average member position to the bar center.
    pub(crate) offset: Vec3,
}

/// The selection decal shape of a squad (protosquad `<Selection>`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct DecalShape {
    /// Half width (`RadiusX`).
    pub(crate) radius_x: f32,
    /// Half length (`RadiusZ`).
    pub(crate) radius_z: f32,
    /// Height after the terrain conform (`YOffset`).
    pub(crate) y_offset: f32,
    /// Offset along the squad forward direction (`ZOffset`).
    pub(crate) z_offset: f32,
}

/// The selection and pick data of one proto object.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ObjectInfo {
    pub(crate) select_type: SelectType,
    pub(crate) pick_priority: PickPriority,
    /// If true, the `GotoType` is Infantry, Vehicle, Air or Hero.
    pub(crate) army_goto_type: bool,
    /// `PickRadius`. Zero selects the obstruction radius.
    pub(crate) pick_radius: f32,
    /// `PickOffset`: the height of the pick center above the unit position.
    pub(crate) pick_offset: f32,
    /// Flag `MustOwnToSelect`.
    pub(crate) must_own_to_select: bool,
    /// `SubSelectSort`, the key of the double click and of the squad order.
    pub(crate) sub_select_sort: i32,
    /// The proto maxima: hit points, shield points and ammunition.
    pub(crate) maxima: [f32; 3],
    /// The object types (`Building`, `BuildingSocket` and others).
    pub(crate) object_types: Vec<String>,
    /// The `HPBar` style of a unit bar, if the object has the `HasHPBar` flag.
    pub(crate) unit_hp_bar: Option<String>,
    /// `ObstructionRadiusY`. A unit bar is twice this height above the unit.
    pub(crate) obstruction_radius_y: f32,
}

/// The overlay data of one protosquad, explicit or implicit.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SquadInfo {
    /// `SubSelectSort` (`protosquad+0xE0`).
    pub(crate) sub_select_sort: i32,
    pub(crate) decal: DecalShape,
    /// The HP bar, if the protosquad has one.
    pub(crate) hp_bar: Option<BarStyle>,
    /// Flag `AlwaysShowHPBar`.
    pub(crate) always_show_hp_bar: bool,
    /// Flag `AlwaysRenderSelectionDecal`.
    pub(crate) always_render_selection_decal: bool,
    /// The member protos and their counts.
    pub(crate) members: Vec<(String, i32)>,
}

/// The proto data of the client, by proto name.
#[derive(Clone, Debug, Default)]
pub(crate) struct ClientCatalog {
    objects: HashMap<String, ObjectInfo>,
    squads: HashMap<String, SquadInfo>,
    /// `SizeX` and `SizeY` of each `hpbars.xml` style.
    hp_bar_sizes: HashMap<String, [f32; 2]>,
    /// The `selection` colour of each `skirmish` colour number of `playercolors.xml`.
    selection_colors: Vec<Option<[u8; 3]>>,
}

impl ClientCatalog {
    /// Reads the objects and the protosquads of `database`.
    ///
    /// Like `BProtoSquad_initImplicit` (`0x1403534B0`), an object without a protosquad of the
    /// same name gets an implicit protosquad: one member, the object `SelectedRadiusX/Z` as the
    /// decal size, the object `SubSelectSort`, and the object HP bar with the default offset of
    /// twice `ObstructionRadiusY` above the object.
    pub(crate) fn new(database: &Database) -> Self {
        let objects = database
            .objects
            .iter()
            .map(|object| (object.name.clone(), object_info(object)))
            .collect();
        let mut squads: HashMap<String, SquadInfo> = database
            .squads
            .iter()
            .map(|squad| (squad.name.clone(), squad_info(squad)))
            .collect();
        for object in &database.objects {
            squads
                .entry(object.name.clone())
                .or_insert_with(|| implicit_squad_info(object));
        }
        let hp_bar_sizes = database
            .hp_bars
            .iter()
            .flat_map(|definition| &definition.hp_bars)
            .map(|bar| (bar.name.clone(), [bar.size_x, bar.size_y]))
            .collect();
        // Skirmish games use the `skirmish` set (`BWorld_init`, `0x1405C1E30`).
        let selection_colors = database
            .player_colors
            .as_ref()
            .and_then(|colors| colors.skirmish.as_ref())
            .map(|set| {
                (0..set.colors.len().max(16))
                    .map(|num| {
                        set.color(num).map(|color| {
                            [
                                color.selection.red,
                                color.selection.green,
                                color.selection.blue,
                            ]
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            objects,
            squads,
            hp_bar_sizes,
            selection_colors,
        }
    }

    /// Returns the data of the proto object `name`.
    pub(crate) fn object(&self, name: &str) -> Option<&ObjectInfo> {
        self.objects.get(name)
    }

    /// Returns the data of the protosquad `name`.
    pub(crate) fn squad(&self, name: &str) -> Option<&SquadInfo> {
        self.squads.get(name)
    }

    /// Returns the `SizeX` and `SizeY` of the `hpbars.xml` style `name`.
    pub(crate) fn hp_bar_size(&self, name: &str) -> Option<[f32; 2]> {
        self.hp_bar_sizes.get(name).copied()
    }

    /// Returns the selection colour of the player colour number `color` (`Player::color`), the
    /// colour of `BWorld_getPlayerColor(player, 1)` (`0x1405D4150`).
    pub(crate) fn selection_color(&self, color: u8) -> Option<[u8; 3]> {
        self.selection_colors
            .get(usize::from(color))
            .copied()
            .flatten()
    }

    /// Adds an object. Tests use it to build a synthetic catalog.
    #[cfg(test)]
    pub(crate) fn insert_object(&mut self, name: &str, info: ObjectInfo) {
        self.objects.insert(name.to_owned(), info);
    }

    /// Adds a protosquad. Tests use it to build a synthetic catalog.
    #[cfg(test)]
    pub(crate) fn insert_squad(&mut self, name: &str, info: SquadInfo) {
        self.squads.insert(name.to_owned(), info);
    }

    /// Adds a style size. Tests use it to build a synthetic catalog.
    #[cfg(test)]
    pub(crate) fn insert_hp_bar_size(&mut self, name: &str, size: [f32; 2]) {
        self.hp_bar_sizes.insert(name.to_owned(), size);
    }

    /// Sets a selection colour. Tests use it to build a synthetic catalog.
    #[cfg(test)]
    pub(crate) fn set_selection_color(&mut self, color: u8, rgb: [u8; 3]) {
        let index = usize::from(color);
        if self.selection_colors.len() <= index {
            self.selection_colors.resize(index + 1, None);
        }
        self.selection_colors[index] = Some(rgb);
    }
}

/// Returns `true` if `flags` holds `flag`.
fn has_flag(flags: &[String], flag: &str) -> bool {
    flags.iter().any(|value| value.eq_ignore_ascii_case(flag))
}

fn object_info(object: &ProtoObject) -> ObjectInfo {
    ObjectInfo {
        select_type: SelectType::from_name(object.select_type.as_deref()),
        pick_priority: PickPriority::from_name(object.pick_priority.as_deref()),
        army_goto_type: goto_type_is_army(object.goto_type.as_deref()),
        pick_radius: object.pick_radius.unwrap_or(0.0),
        pick_offset: object.pick_offset.unwrap_or(0.0),
        must_own_to_select: has_flag(&object.flags, "MustOwnToSelect"),
        sub_select_sort: object.sub_select_sort.unwrap_or(DEFAULT_SUB_SELECT_SORT),
        maxima: [
            object.hitpoints.unwrap_or(0.0),
            object.shieldpoints.unwrap_or(0.0),
            object.ammo_max.unwrap_or(0.0),
        ],
        object_types: object.object_types.clone(),
        unit_hp_bar: object
            .hp_bar
            .as_ref()
            .filter(|_| has_flag(&object.flags, "HasHPBar"))
            .and_then(|bar| bar.style.clone()),
        obstruction_radius_y: object.obstruction_radius_y.unwrap_or(0.0),
    }
}

fn squad_info(squad: &ProtoSquad) -> SquadInfo {
    let selection = squad.selection.clone().unwrap_or_default();
    SquadInfo {
        sub_select_sort: squad.sub_select_sort.unwrap_or(DEFAULT_SUB_SELECT_SORT),
        decal: DecalShape {
            radius_x: selection.radius_x.unwrap_or(0.0),
            radius_z: selection.radius_z.unwrap_or(0.0),
            y_offset: selection.y_offset.unwrap_or(0.0),
            z_offset: selection.z_offset.unwrap_or(0.0),
        },
        hp_bar: squad.hp_bar.as_ref().and_then(|bar| {
            Some(BarStyle {
                name: bar.style.clone()?,
                size: [bar.size_x.unwrap_or(0.0), bar.size_y.unwrap_or(0.0)],
                offset: bar
                    .offset
                    .as_ref()
                    .map_or(Vec3::ZERO, |offset| Vec3::new(offset.x, offset.y, offset.z)),
            })
        }),
        always_show_hp_bar: has_flag(&squad.flags, "AlwaysShowHPBar"),
        always_render_selection_decal: has_flag(&squad.flags, "AlwaysRenderSelectionDecal"),
        members: squad
            .units
            .as_ref()
            .map(|units| {
                units
                    .entries
                    .iter()
                    .map(|entry| (entry.proto_object.clone(), entry.count))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn implicit_squad_info(object: &ProtoObject) -> SquadInfo {
    let hp_bar = object
        .hp_bar
        .as_ref()
        .filter(|_| has_flag(&object.flags, "HasHPBar"))
        .and_then(|bar| {
            let offset = bar
                .offset
                .as_ref()
                .map_or(Vec3::ZERO, |offset| Vec3::new(offset.x, offset.y, offset.z));
            // `BProtoSquad_initImplicit`: a zero offset becomes twice `ObstructionRadiusY`.
            let offset = if offset == Vec3::ZERO {
                Vec3::new(0.0, object.obstruction_radius_y.unwrap_or(0.0) * 2.0, 0.0)
            } else {
                offset
            };
            Some(BarStyle {
                name: bar.style.clone()?,
                size: [bar.size_x.unwrap_or(0.0), bar.size_y.unwrap_or(0.0)],
                offset,
            })
        });
    SquadInfo {
        sub_select_sort: object.sub_select_sort.unwrap_or(DEFAULT_SUB_SELECT_SORT),
        decal: DecalShape {
            radius_x: object.selected_radius_x.unwrap_or(0.0),
            radius_z: object.selected_radius_z.unwrap_or(0.0),
            y_offset: 0.0,
            z_offset: 0.0,
        },
        hp_bar,
        always_show_hp_bar: false,
        always_render_selection_decal: false,
        members: vec![(object.name.clone(), 1)],
    }
}
