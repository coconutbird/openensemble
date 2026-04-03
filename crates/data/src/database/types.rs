//! Data type definitions for all game database entities.

/// Civilization definition — matches BCiv struct layout from BCiv__parseFromXml (0x140193000).
#[derive(Debug, Clone, Default)]
pub struct Civilization {
    pub id: u32,
    /// "Name" tag — civ internal name
    pub name: String,
    /// "SoundBank" tag — audio bank name (not sim-critical)
    pub sound_bank: String,
    /// "UIControlBackground" tag — wide string for UI (not sim-critical)
    pub ui_control_background: String,
    /// "DisplayNameID" tag — localized display name ID (resolved to int)
    pub display_name_id: i32,
    /// "CivTech" tag — tech ID applied to this civ (resolved via BStringTable)
    pub civ_tech: i32,
    /// "CommandAckObject" tag — proto object ID for command acknowledgement
    pub command_ack_object: i32,
    /// "RallyPointObject" tag — proto object ID for rally point
    pub rally_point_object: i32,
    /// "LocalRallyPointObject" tag — proto object ID for local rally point
    pub local_rally_point_object: i32,
    /// "ExpandHull" tag — float
    pub expand_hull: f32,
    /// "TerrainPushOff" tag — float
    pub terrain_push_off: f32,
    /// "BuildingMagnetRange" tag — float
    pub building_magnet_range: f32,
    /// "Transport" tag — proto object ID for transport
    pub transport: i32,
    /// "TransportTrigger" tag — proto object ID for transport trigger
    pub transport_trigger: i32,
    /// "LeaderMenuNameID" tag — localized leader menu name ID (resolved to int)
    pub leader_menu_name_id: i32,
    /// "Alpha" attribute on <Civ> node — int
    pub alpha: i32,
    /// "PowerFromHero" tag — bool
    pub power_from_hero: bool,
}

/// Leader definition — matches BLeader struct layout from BLeader__parseFromXml (0x140253550).
#[derive(Debug, Clone, Default)]
pub struct Leader {
    pub id: u32,
    /// +0: "Name" — leader internal name (via attribute on reader)
    pub name: String,
    /// +48: "Icon" — icon path string
    pub icon: String,
    /// +64: "FlashImg" — flash image path string
    pub flash_img: String,
    /// +80: "FlashPortrait" — flash portrait path string
    pub flash_portrait: String,
    /// +96: "UIControlBackground" — wide string (UI only)
    pub ui_control_background: String,
    /// +128: "Resource" — per-resource-type overrides (resource_type_id -> amount)
    pub resource_overrides: Vec<LeaderResourceOverride>,
    /// +152: "StartingSquad" — starting squad entries with offset/fly-in
    pub starting_squads: Vec<LeaderStartingSquad>,
    /// +176: "StartingUnit" — starting unit entries with offset/build-other/dopple
    pub starting_units: Vec<LeaderStartingUnit>,
    /// +208: "RallyPointOffset" — rally point offset vector [x, y, z, w]
    pub rally_point_offset: [f32; 4],
    /// +224: "Pop" — population overrides per pop type
    pub pop_overrides: Vec<LeaderPopOverride>,
    /// +240: "SupportPower" — support power definitions
    pub support_powers: Vec<LeaderSupportPower>,
    /// +264: "Civ" — resolved civ ID (-1 if not found)
    pub civ_id: i32,
    /// +268: "Tech" — resolved tech ID (-1 if not found)
    pub tech_id: i32,
    /// +272: "Power" — resolved power ID (-1 if not found)
    pub power_id: i32,
    /// +276: "NameID" — resolved loc string ID
    pub name_id: i32,
    /// +280: "DescriptionID" — resolved loc string ID
    pub description_id: i32,
    /// +284: "RepairRate" — float
    pub repair_rate: f32,
    /// +288: "RepairDelay" — stored as (float * 1000) as i32 (milliseconds)
    pub repair_delay: i32,
    /// +292: "RepairCost" — resource cost (type + amount)
    pub repair_cost: Vec<LeaderResourceOverride>,
    /// +312: "RepairTime" — float
    pub repair_time: f32,
    /// +316: "ReverseHotDropCost" — resource cost (type + amount)
    pub reverse_hot_drop_cost: Vec<LeaderResourceOverride>,
    /// +336: "Test" attribute — bool
    pub test: bool,
    /// +337: "Random" attribute — bool
    pub random: bool,
    /// +338: "FlashCivID" — i8
    pub flash_civ_id: i8,
    /// +339: "StatsID" attribute — i8
    pub stats_id: i8,
    /// +340: "LeaderPickerOrder" attribute — i8
    pub leader_picker_order: i8,
    /// +341: "DefaultPlayerSlotFlags" — i8 (parsed from string via strtol)
    pub default_player_slot_flags: i8,
    /// +342: Resource override flags (bitfield, 1 byte)
    pub resource_flags: u8,
}

/// A starting squad entry for a leader.
#[derive(Debug, Clone, Default)]
pub struct LeaderStartingSquad {
    /// Proto squad ID (resolved from name)
    pub squad_id: i32,
    /// Offset position [x, y, z, w]
    pub offset: [f32; 4],
    /// Whether the squad flies in
    pub fly_in: bool
}

/// A starting unit entry for a leader.
#[derive(Debug, Clone, Default)]
pub struct LeaderStartingUnit {
    /// Proto object ID (resolved from name)
    pub object_id: i32,
    /// Offset position [x, y, z, w]
    pub offset: [f32; 4],
    /// Proto object ID for "BuildOther" (-1 if none)
    pub build_other: i32,
    /// Whether to dopple on start
    pub dopple_on_start: bool,
}

/// A support power entry for a leader.
#[derive(Debug, Clone, Default)]
pub struct LeaderSupportPower {
    /// Icon location index
    pub icon_location: i32,
    /// Tech prereq ID (-1 if none)
    pub tech_prereq: i32,
    /// Power IDs associated with this support power slot
    pub power_ids: Vec<i32>,
}

/// A population override for a leader.
#[derive(Debug, Clone, Default)]
pub struct LeaderPopOverride {
    /// Pop type ID (resolved from name)
    pub pop_type: i32,
    /// Override amount (float)
    pub amount: f32,
    /// Override max cap
    pub max: i32,
}

/// A resource override for a leader (used for Resource, RepairCost, ReverseHotDropCost).
#[derive(Debug, Clone, Default)]
pub struct LeaderResourceOverride {
    /// Resource type ID (resolved from name)
    pub resource_type: i32,
    /// Amount (float)
    pub amount: f32,
}

/// Power/ability definition.
#[derive(Debug, Clone, Default)]
pub struct Power {
    pub id: u32,
    pub name: String,
    pub display_name: String,
    pub icon: String,
    pub tech_prereq: String,
    pub power_type: String,
    pub auto_recharge: f32,
    pub use_limit: i32,
}

/// Ability definition.
#[derive(Debug, Clone, Default)]
pub struct Ability {
    pub id: u32,
    pub name: String,
    pub display_name: String,
    pub ability_type: String,
    pub recover_time: f32,
    pub movement_modifier: f32,
}

/// Weapon type definition.
#[derive(Debug, Clone, Default)]
pub struct WeaponType {
    pub id: u32,
    pub name: String,
    pub death_animation: String,
    pub damage_modifiers: Vec<DamageModifier>,
}

/// Per-damage-type modifier on a weapon type (sub_1405C01A0).
#[derive(Debug, Clone, Default)]
pub struct DamageModifier {
    pub damage_type: String,
    pub modifier: f32,
    pub rating: i32,
    pub reflect_damage_factor: i32,
    pub bowlable: bool,
    pub rammable: bool,
}

/// Game mode definition (sub_140240E70).
/// Engine parses: Name, WorldScript, PlayerScript, NPC, Tech,
/// DisplayNameID, DescriptionID, LongDescriptionID
/// Plus attributes: Locked, dlc
#[derive(Debug, Clone)]
pub struct GameMode {
    pub id: u32,
    pub name: String,
    pub world_script: String,
    pub player_script: String,
    pub npc: String,
    pub tech_id: i32,
    pub display_name_id: i32,
    pub description_id: i32,
    pub long_description_id: i32,
    pub locked: bool,
    pub dlc: bool,
}

impl Default for GameMode {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            world_script: String::new(),
            player_script: String::new(),
            npc: String::new(),
            tech_id: -1,
            display_name_id: -1,
            description_id: -1,
            long_description_id: -1,
            locked: false,
            dlc: false,
        }
    }
}

/// Damage type definition.
#[derive(Debug, Clone, Default)]
pub struct DamageType {
    pub id: u32,
    pub name: String,
    pub shielded: bool,
    pub attenuates: bool,
}

/// Population type definition from gamedata.xml.
#[derive(Debug, Clone, Default)]
pub struct PopDefinition {
    pub id: u32,
    pub name: String,
    pub max: i32,
}

/// A single player color entry — 5 DWORD color values matching BPlayerColor layout.
/// Each field stores a packed ARGB color, or -1 if not set.
/// Parsed by BPlayerColor__parseFromXml (0x1402FFDD0).
#[derive(Debug, Clone)]
pub struct PlayerColor {
    /// "objects" attribute — color for in-game objects
    pub objects: i32,
    /// "corpse" attribute — color for corpses
    pub corpse: i32,
    /// "selection" attribute — color for selection circles
    pub selection: i32,
    /// "minimap" attribute — color for minimap dots
    pub minimap: i32,
    /// "ui" attribute — color for UI elements
    pub ui: i32,
}

impl Default for PlayerColor {
    fn default() -> Self {
        Self {
            objects: -1,
            corpse: -1,
            selection: -1,
            minimap: -1,
            ui: -1,
        }
    }
}

/// A named color set (e.g. "spc" or "skirmish").
/// Contains up to 16 player colors and 4 friend-or-foe colors.
/// Matches the engine's storage at `qword_141519110[40 * setIndex + 5807]`.
#[derive(Debug, Clone)]
pub struct PlayerColorSet {
    /// Per-player colors (index 0..15, "color" child nodes with "num" attribute)
    pub player_colors: [PlayerColor; 16],
    /// "friendOrFoeSelf" color
    pub friend_or_foe_self: PlayerColor,
    /// "friendOrFoeAlly" color
    pub friend_or_foe_ally: PlayerColor,
    /// "friendOrFoeNeutral" color
    pub friend_or_foe_neutral: PlayerColor,
    /// "friendOrFoeEnemy" color
    pub friend_or_foe_enemy: PlayerColor,
}

impl Default for PlayerColorSet {
    fn default() -> Self {
        Self {
            player_colors: std::array::from_fn(|_| PlayerColor::default()),
            friend_or_foe_self: PlayerColor::default(),
            friend_or_foe_ally: PlayerColor::default(),
            friend_or_foe_neutral: PlayerColor::default(),
            friend_or_foe_enemy: PlayerColor::default(),
        }
    }
}

/// Per-civilization player color overrides.
/// Each civ can define up to 9 per-player color values.
/// Stored at `qword_141519110[5907] + 9 * civID + playerNum`.
#[derive(Debug, Clone)]
pub struct CivPlayerColors {
    /// Civ ID (0-based, max 4)
    pub civ_id: i32,
    /// Per-player color values (index 0..8), -1 if not set
    pub colors: [i32; 9],
}

impl Default for CivPlayerColors {
    fn default() -> Self {
        Self {
            civ_id: -1,
            colors: [-1; 9],
        }
    }
}

/// Top-level player color data from playercolors.xml.
/// Matches BDatabase__loadPlayerColors (0x1401F9560).
#[derive(Debug, Clone, Default)]
pub struct PlayerColorData {
    /// "spc" (campaign) color set
    pub spc: PlayerColorSet,
    /// "skirmish" color set
    pub skirmish: PlayerColorSet,
    /// Per-civ color overrides (up to 5 civs)
    pub civ_colors: Vec<CivPlayerColors>,
}

/// Terrain tile type definition from terrainTileTypes.xml.
#[derive(Debug, Clone, Default)]
pub struct TerrainTileType {
    pub id: u32,
    pub name: String,
}

/// Object type grouping from objecttypes.xml.
#[derive(Debug, Clone, Default)]
pub struct ObjectTypeEntry {
    pub id: u32,
    pub name: String,
}

/// Global game data constants from gamedata.xml.
/// These are simulation-wide settings that control gameplay balance.
#[derive(Debug, Clone)]
pub struct GameData {
    // --- Resources ---
    pub supplies_deductable: bool,
    pub power_deductable: bool,
    pub leader_power_charge_deductable: bool,
    pub supplies_rate: f32,
    pub power_rate: f32,
    pub leader_power_charge_rate: f32,

    // --- Population ---
    pub pops: Vec<PopDefinition>,

    // --- Combat ---
    pub garrison_damage_multiplier: f32,
    pub construction_damage_multiplier: f32,
    pub capture_decay_rate: f32,
    pub projectile_gravity: f32,
    pub projectile_tumble_rate: f32,
    pub height_bonus_damage: f32,
    pub attack_rating_multiplier: f32,
    pub defense_rating_multiplier: f32,
    pub chance_to_rocket: f32,
    pub damage_bank_timer: f32,
    pub max_damage_bank_pct_adjust: f32,
    pub overrun_distance: f32,
    pub overrun_min_vel: f32,
    pub overrun_jump_force: f32,

    // --- Leashing ---
    pub squad_leash_length: f32,
    pub squad_aggro_length: f32,
    pub unit_leash_length: f32,

    // --- Shields & Cloaking ---
    pub shield_regen_delay: f32,
    pub shield_regen_time: f32,
    pub cloaking_delay: f32,
    pub re_cloak_delay: f32,
    pub cloak_detect_frequency: f32,

    // --- Heroes ---
    pub hero_downed_los: f32,
    pub hero_hp_regen_time: f32,
    pub hero_revival_distance: f32,
    pub hero_percent_hp_revival_threshold: f32,

    // --- Transport ---
    pub transport_max: i32,
    pub transport_incoming_height: f32,
    pub transport_outgoing_height: f32,
    pub transport_pickup_height: f32,
    pub transport_dropoff_height: f32,

    // --- Economy ---
    pub tribute_amount: f32,
    pub tribute_cost: f32,
    pub unsc_supply_pad_bonus: f32,
    pub unsc_supply_pad_break_even_point: f32,
    pub cov_supply_pad_bonus: f32,
    pub cov_supply_pad_break_even_point: f32,
    pub leader_power_charge_resource: i32,
    pub leader_power_charge_rate_value: f32,
    pub recycle_refund_rate: f32,
    pub base_rebuild_timer: f32,
    pub coop_resource_split_rate: f32,

    // --- Misc ---
    pub game_over_delay: f32,
    pub max_num_corpses: i32,
    pub building_self_destruct_time: f32,
    pub damage_received_xp_factor: f32,
    pub air_strike_loiter_time: f32,

    // --- Cryo ---
    pub default_cryo_points: f32,
    pub default_thaw_speed: f32,
    pub frozen_damage_modifier: f32,
    pub freezing_speed_modifier: f32,
    pub freezing_damage_modifier: f32,
    pub time_frozen_to_thaw: f32,
    pub time_freezing_to_thaw: f32,
}

impl Default for GameData {
    fn default() -> Self {
        Self {
            supplies_deductable: true,
            power_deductable: true,
            leader_power_charge_deductable: true,
            supplies_rate: 1.0,
            power_rate: 1.0,
            leader_power_charge_rate: 1.0,
            pops: Vec::new(),
            garrison_damage_multiplier: 1.0,
            construction_damage_multiplier: 1.0,
            capture_decay_rate: 0.0,
            projectile_gravity: 9.81,
            projectile_tumble_rate: 0.0,
            height_bonus_damage: 0.0,
            attack_rating_multiplier: 1.0,
            defense_rating_multiplier: 1.0,
            chance_to_rocket: 0.0,
            damage_bank_timer: 0.0,
            max_damage_bank_pct_adjust: 0.0,
            overrun_distance: 0.0,
            overrun_min_vel: 0.0,
            overrun_jump_force: 0.0,
            squad_leash_length: 0.0,
            squad_aggro_length: 0.0,
            unit_leash_length: 0.0,
            shield_regen_delay: 0.0,
            shield_regen_time: 0.0,
            cloaking_delay: 0.0,
            re_cloak_delay: 0.0,
            cloak_detect_frequency: 0.0,
            hero_downed_los: 0.0,
            hero_hp_regen_time: 0.0,
            hero_revival_distance: 0.0,
            hero_percent_hp_revival_threshold: 0.0,
            transport_max: 0,
            transport_incoming_height: 0.0,
            transport_outgoing_height: 0.0,
            transport_pickup_height: 0.0,
            transport_dropoff_height: 0.0,
            tribute_amount: 0.0,
            tribute_cost: 0.0,
            unsc_supply_pad_bonus: 0.0,
            unsc_supply_pad_break_even_point: 0.0,
            cov_supply_pad_bonus: 0.0,
            cov_supply_pad_break_even_point: 0.0,
            leader_power_charge_resource: 0,
            leader_power_charge_rate_value: 0.0,
            recycle_refund_rate: 0.0,
            base_rebuild_timer: 0.0,
            coop_resource_split_rate: 0.0,
            game_over_delay: 0.0,
            max_num_corpses: 0,
            building_self_destruct_time: 0.0,
            damage_received_xp_factor: 0.0,
            air_strike_loiter_time: 0.0,
            default_cryo_points: 0.0,
            default_thaw_speed: 0.0,
            frozen_damage_modifier: 1.0,
            freezing_speed_modifier: 1.0,
            freezing_damage_modifier: 1.0,
            time_frozen_to_thaw: 0.0,
            time_freezing_to_thaw: 0.0,
        }
    }
}
