//! Parsers for civs, leaders, powers, abilities, weapon types, game modes,
//! damage types, gamedata, object types, terrain tile types, and player colors.

use super::DatabaseError;
use super::GameDatabase;
use super::types::*;
use crate::xmb::Document;

impl GameDatabase {
    /// Parse civs.xml — matches BCiv__parseFromXml (0x140193000).
    ///
    /// Field offsets verified against decompiled struct layout:
    ///   +0  Name (BString)
    ///   +16 SoundBank (BString)
    ///   +32 UIControlBackground (BWideString)
    ///   +52 DisplayNameID (int, loc string ID)
    ///   +56 CivTech (int, tech ID via BStringTable)
    ///   +60 CommandAckObject (int, proto object ID)
    ///   +64 RallyPointObject (int, proto object ID)
    ///   +68 LocalRallyPointObject (int, proto object ID)
    ///   +72 ExpandHull (float)
    ///   +76 TerrainPushOff (float)
    ///   +80 BuildingMagnetRange (float)
    ///   +84 Transport (int, proto object ID)
    ///   +88 TransportTrigger (int, proto object ID)
    ///   +92 LeaderMenuNameID (int, loc string ID)
    ///   +96 Alpha (int, from attribute)
    ///   +100 PowerFromHero (bool)
    pub(super) fn parse_civs(
        &mut self,
        xmb: &Document,
        _filename: &str,
    ) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: u32 = 0;

        for node in &root.children {
            if node.name == "Civ" {
                let mut civ = Civilization {
                    id: id_counter,
                    ..Default::default()
                };
                id_counter += 1;

                // Alpha attribute on <Civ> node (offset +96)
                if let Some(alpha_attr) = node.get_attribute("Alpha") {
                    civ.alpha = alpha_attr.value_string().parse::<i32>().unwrap_or(0);
                }

                for child in &node.children {
                    match child.name.as_str() {
                        "Name" => civ.name = child.text_string(),
                        "CivTech" => {
                            // Original resolves via BStringTable to tech ID.
                            // We store the string name; ID resolution happens later.
                            civ.civ_tech = self.resolve_tech_id(&child.text_string());
                        }
                        "DisplayNameID" => {
                            civ.display_name_id =
                                self.resolve_loc_string_id(&child.text_string());
                        }
                        "CommandAckObject" => {
                            civ.command_ack_object =
                                self.resolve_proto_object_id(&child.text_string());
                        }
                        "RallyPointObject" => {
                            civ.rally_point_object =
                                self.resolve_proto_object_id(&child.text_string());
                        }
                        "LocalRallyPointObject" => {
                            civ.local_rally_point_object =
                                self.resolve_proto_object_id(&child.text_string());
                        }
                        "Transport" => {
                            civ.transport =
                                self.resolve_proto_object_id(&child.text_string());
                        }
                        "TransportTrigger" => {
                            civ.transport_trigger =
                                self.resolve_proto_object_id(&child.text_string());
                        }
                        "ExpandHull" => {
                            civ.expand_hull =
                                child.text_string().parse::<f32>().unwrap_or(0.0);
                        }
                        "TerrainPushOff" => {
                            civ.terrain_push_off =
                                child.text_string().parse::<f32>().unwrap_or(0.0);
                        }
                        "BuildingMagnetRange" => {
                            civ.building_magnet_range =
                                child.text_string().parse::<f32>().unwrap_or(0.0);
                        }
                        "SoundBank" => {
                            civ.sound_bank = child.text_string();
                        }
                        "LeaderMenuNameID" => {
                            civ.leader_menu_name_id =
                                self.resolve_loc_string_id(&child.text_string());
                        }
                        "PowerFromHero" => {
                            let t = child.text_string();
                            civ.power_from_hero =
                                t.eq_ignore_ascii_case("true") || t == "1";
                        }
                        "UIControlBackground" => {
                            civ.ui_control_background = child.text_string();
                        }
                        _ => {}
                    }
                }

                let id = civ.id;
                let name = civ.name.clone();
                self.civs.insert(id, civ);
                self.civs_by_name.insert(name, id);
            }
        }

        Ok(())
    }

    /// Parse leaders from leaders.xml.
    /// Matches BLeader__parseFromXml (0x140253550) and BDatabase__loadLeaders (0x1401F14E0).
    ///
    /// BLeader layout (offsets from decompilation):
    ///   +0   Name (string)
    ///   +48  Icon (string)
    ///   +64  FlashImg (string)
    ///   +80  FlashPortrait (string)
    ///   +96  UIControlBackground (wide string)
    ///   +128 Resource cost overrides (BResourceCost)
    ///   +152 StartingSquad array (BDynamicArray<32 bytes>)
    ///   +176 StartingUnit array (BDynamicArray<32 bytes>)
    ///   +208 RallyPointOffset (BVector, 16 bytes)
    ///   +224 Pop overrides array (BDynamicArray<12 bytes>)
    ///   +240 SupportPower (via BLeader__addSupportPower)
    ///   +264 CivID (i32)
    ///   +268 TechID (i32)
    ///   +272 PowerID (i32)
    ///   +276 NameID (i32, loc string)
    ///   +280 DescriptionID (i32, loc string)
    ///   +284 RepairRate (float)
    ///   +288 RepairDelay (i32, ms = float * 1000)
    ///   +292 RepairCost (BResourceCost)
    ///   +312 RepairTime (float)
    ///   +316 ReverseHotDropCost (BResourceCost)
    ///   +336 Test (bool)
    ///   +337 Random (bool)
    ///   +338 FlashCivID (i8)
    ///   +339 StatsID (i8)
    ///   +340 LeaderPickerOrder (i8)
    ///   +341 DefaultPlayerSlotFlags (i8)
    ///   +342 Resource flags (u8 bitfield)
    pub(super) fn parse_leaders(
        &mut self,
        xmb: &Document,
        _filename: &str,
    ) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: u32 = 0;

        for node in &root.children {
            if node.name == "Leader" {
                let mut leader = Leader {
                    id: id_counter,
                    civ_id: -1,
                    tech_id: -1,
                    power_id: -1,
                    name_id: -1,
                    description_id: -1,
                    ..Default::default()
                };
                id_counter += 1;

                // The engine reads Name and Icon via the reader as top-level
                // attribute reads before entering the child loop. In XMB these
                // appear as child nodes.
                for child in &node.children {
                    match child.name.as_str() {
                        "Name" => leader.name = child.text_string(),
                        "Icon" => leader.icon = child.text_string(),
                        "FlashImg" => leader.flash_img = child.text_string(),
                        "FlashPortrait" => leader.flash_portrait = child.text_string(),
                        "UIControlBackground" => {
                            leader.ui_control_background = child.text_string();
                        }
                        "Tech" => {
                            let text = child.text_string();
                            leader.tech_id = self.resolve_tech_id(&text);
                        }
                        "Civ" => {
                            let text = child.text_string();
                            leader.civ_id = self.resolve_civ_id(&text);
                        }
                        "Power" => {
                            let text = child.text_string();
                            leader.power_id = self.resolve_power_id(&text);
                        }
                        "NameID" => {
                            let text = child.text_string();
                            leader.name_id = self.resolve_loc_string_id(&text);
                        }
                        "DescriptionID" => {
                            let text = child.text_string();
                            leader.description_id = self.resolve_loc_string_id(&text);
                        }
                        "FlashCivID" => {
                            let v: i32 = child.text_string().parse().unwrap_or(0);
                            leader.flash_civ_id = v.clamp(-128, 127) as i8;
                        }
                        "RepairRate" => {
                            leader.repair_rate =
                                child.text_string().parse::<f32>().unwrap_or(0.0);
                        }
                        "RepairDelay" => {
                            let v: f32 =
                                child.text_string().parse::<f32>().unwrap_or(0.0);
                            leader.repair_delay = (v * 1000.0) as i32;
                        }
                        "RepairTime" => {
                            leader.repair_time =
                                child.text_string().parse::<f32>().unwrap_or(0.0);
                        }
                        "RepairCost" => {
                            Self::parse_leader_resource_cost(
                                child,
                                &mut leader.repair_cost,
                            );
                        }
                        "ReverseHotDropCost" => {
                            Self::parse_leader_resource_cost(
                                child,
                                &mut leader.reverse_hot_drop_cost,
                            );
                        }
                        "RallyPointOffset" => {
                            leader.rally_point_offset =
                                Self::parse_vector4_text(child);
                        }
                        "SupportPower" => {
                            if let Some(sp) = self.parse_leader_support_power(child)
                            {
                                leader.support_powers.push(sp);
                            }
                        }
                        "StartingSquad" => {
                            let text = child.text_string();
                            let squad_id = self.resolve_proto_squad_id(&text);
                            if squad_id >= 0 {
                                let mut entry = LeaderStartingSquad {
                                    squad_id,
                                    ..Default::default()
                                };
                                // Parse "Offset" attribute
                                if let Some(attr) = child.get_attribute("Offset") {
                                    entry.offset =
                                        Self::parse_vector4_attr(&attr.value_string());
                                }
                                // Parse "FlyIn" attribute
                                if let Some(attr) = child.get_attribute("FlyIn") {
                                    let t = attr.value_string();
                                    entry.fly_in =
                                        t.eq_ignore_ascii_case("true") || t == "1";
                                }
                                leader.starting_squads.push(entry);
                            }
                        }
                        "StartingUnit" => {
                            let text = child.text_string();
                            let object_id = self.resolve_proto_object_id(&text);
                            if object_id >= 0 {
                                let mut entry = LeaderStartingUnit {
                                    object_id,
                                    build_other: -1,
                                    ..Default::default()
                                };
                                // Parse "Offset" attribute
                                if let Some(attr) = child.get_attribute("Offset") {
                                    entry.offset =
                                        Self::parse_vector4_attr(&attr.value_string());
                                }
                                // Parse "BuildOther" attribute
                                if let Some(attr) = child.get_attribute("BuildOther")
                                {
                                    entry.build_other =
                                        self.resolve_proto_object_id(
                                            &attr.value_string(),
                                        );
                                }
                                // Parse "DoppleOnStart" attribute
                                if let Some(attr) =
                                    child.get_attribute("DoppleOnStart")
                                {
                                    let t = attr.value_string();
                                    entry.dopple_on_start =
                                        t.eq_ignore_ascii_case("true") || t == "1";
                                }
                                leader.starting_units.push(entry);
                            }
                        }
                        "Pop" => {
                            if let Some(pop) =
                                self.parse_leader_pop_override(child)
                            {
                                leader.pop_overrides.push(pop);
                            }
                        }
                        "Resource" => {
                            if let Some(res) =
                                Self::parse_leader_resource_override(child)
                            {
                                // Set the resource flag bit
                                if res.resource_type >= 0
                                    && res.resource_type < 8
                                {
                                    leader.resource_flags |=
                                        1 << (res.resource_type % 8);
                                }
                                leader.resource_overrides.push(res);
                            }
                        }
                        _ => {
                            // Handle attributes parsed before the child loop
                            // in the engine: Test, Random, StatsID,
                            // LeaderPickerOrder, DefaultPlayerSlotFlags
                        }
                    }
                }

                // Parse attributes on the <Leader> node itself
                // (engine reads these via the reader before child iteration)
                if let Some(attr) = node.get_attribute("Test") {
                    let t = attr.value_string();
                    leader.test = t.eq_ignore_ascii_case("true") || t == "1";
                }
                if let Some(attr) = node.get_attribute("Random") {
                    let t = attr.value_string();
                    leader.random = t.eq_ignore_ascii_case("true") || t == "1";
                }
                if let Some(attr) = node.get_attribute("StatsID") {
                    let v: i32 = attr.value_string().parse().unwrap_or(0);
                    leader.stats_id = v.clamp(-128, 127) as i8;
                }
                if let Some(attr) = node.get_attribute("LeaderPickerOrder") {
                    let v: i32 = attr.value_string().parse().unwrap_or(0);
                    leader.leader_picker_order = v.clamp(-128, 127) as i8;
                }
                if let Some(attr) = node.get_attribute("DefaultPlayerSlotFlags") {
                    // Engine parses via strtol(str, nullptr, 0)
                    let s = attr.value_string();
                    leader.default_player_slot_flags = if s.starts_with("0x") || s.starts_with("0X")
                    {
                        i8::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16)
                            .unwrap_or(0)
                    } else {
                        s.parse::<i8>().unwrap_or(0)
                    };
                }

                let id = leader.id;
                let name = leader.name.clone();
                self.leaders.insert(id, leader);
                self.leaders_by_name.insert(name, id);
            }
        }

        Ok(())
    }

    /// Parse a "Type" + text-value resource cost node (used by RepairCost,
    /// ReverseHotDropCost). Matches the engine's pattern of reading the "Type"
    /// attribute and then the text float value.
    fn parse_leader_resource_cost(
        node: &crate::xmb::Node,
        out: &mut Vec<LeaderResourceOverride>,
    ) {
        if let Some(type_attr) = node.get_attribute("Type") {
            let type_name = type_attr.value_string();
            // Use hardcoded resource type mapping
            let resource_type = match type_name.as_str() {
                "Supplies" => 0,
                "Power" => 1,
                "LeaderPowerCharge" => 2,
                _ => -1,
            };
            if resource_type >= 0 {
                let amount: f32 = node.text_string().parse().unwrap_or(0.0);
                out.push(LeaderResourceOverride {
                    resource_type,
                    amount,
                });
            }
        }
    }

    /// Parse a Resource override node (has "Type" attribute, text is float amount).
    fn parse_leader_resource_override(
        node: &crate::xmb::Node,
    ) -> Option<LeaderResourceOverride> {
        let type_attr = node.get_attribute("Type")?;
        let type_name = type_attr.value_string();
        let resource_type = match type_name.as_str() {
            "Supplies" => 0,
            "Power" => 1,
            "LeaderPowerCharge" => 2,
            _ => return None,
        };
        let amount: f32 = node.text_string().parse().unwrap_or(0.0);
        Some(LeaderResourceOverride {
            resource_type,
            amount,
        })
    }

    /// Parse a Pop override node (has "Type" attribute, text is float amount,
    /// optional "Max" attribute).
    fn parse_leader_pop_override(
        &self,
        node: &crate::xmb::Node,
    ) -> Option<LeaderPopOverride> {
        let type_attr = node.get_attribute("Type")?;
        let type_name = type_attr.value_string();
        let pop_type = self.resolve_pop_type_id(&type_name);
        if pop_type < 0 {
            return None;
        }
        let amount: f32 = node.text_string().parse().unwrap_or(0.0);
        let max: i32 = node
            .get_attribute("Max")
            .and_then(|a| a.value_string().parse().ok())
            .unwrap_or(0);
        Some(LeaderPopOverride {
            pop_type,
            amount,
            max,
        })
    }

    /// Parse a SupportPower node. Matches the engine's pattern:
    /// - "IconLocation" attribute -> i32
    /// - "TechPrereq" attribute -> resolved tech ID
    /// - Child nodes named "Power" -> resolved power IDs
    fn parse_leader_support_power(
        &self,
        node: &crate::xmb::Node,
    ) -> Option<LeaderSupportPower> {
        let mut sp = LeaderSupportPower {
            icon_location: -1,
            tech_prereq: -1,
            power_ids: Vec::new(),
        };

        if let Some(attr) = node.get_attribute("IconLocation") {
            sp.icon_location = attr.value_string().parse().unwrap_or(-1);
        }
        if let Some(attr) = node.get_attribute("TechPrereq") {
            sp.tech_prereq = self.resolve_tech_id(&attr.value_string());
        }

        // Collect Power child nodes
        for child in &node.children {
            if child.name == "Power" {
                let text = child.text_string();
                let power_id = self.resolve_power_id(&text);
                if power_id >= 0 {
                    sp.power_ids.push(power_id);
                }
            }
        }

        if sp.power_ids.is_empty() {
            return None;
        }
        Some(sp)
    }

    /// Parse a vector4 from a node's text content (e.g. "1.0, 2.0, 3.0").
    fn parse_vector4_text(node: &crate::xmb::Node) -> [f32; 4] {
        let text = node.text_string();
        Self::parse_vector4_attr(&text)
    }

    /// Parse a "x,y,z[,w]" string into [f32; 4].
    fn parse_vector4_attr(text: &str) -> [f32; 4] {
        let parts: Vec<f32> = text
            .split(',')
            .filter_map(|p| p.trim().parse().ok())
            .collect();
        let mut v = [0.0f32; 4];
        for (i, &val) in parts.iter().enumerate().take(4) {
            v[i] = val;
        }
        v
    }

    pub(super) fn parse_powers(
        &mut self,
        xmb: &Document,
        _filename: &str,
    ) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: u32 = 0;

        for node in &root.children {
            if node.name == "Power" {
                let mut power = Power {
                    id: id_counter,
                    ..Default::default()
                };
                id_counter += 1;

                if let Some(name_attr) = node.get_attribute("name") {
                    power.name = name_attr.value_string();
                }

                for child in &node.children {
                    match child.name.as_str() {
                        "Name" => power.name = child.text_string(),
                        "DisplayNameID" => power.display_name = child.text_string(),
                        "Icon" => power.icon = child.text_string(),
                        "TechPrereq" => power.tech_prereq = child.text_string(),
                        "PowerType" => power.power_type = child.text_string(),
                        "AutoRecharge" => {
                            power.auto_recharge = child.text_string().parse().unwrap_or(0.0)
                        }
                        "UseLimit" => power.use_limit = child.text_string().parse().unwrap_or(-1),
                        _ => {}
                    }
                }

                let id = power.id;
                let name = power.name.clone();
                self.powers.insert(id, power);
                self.powers_by_name.insert(name, id);
            }
        }

        Ok(())
    }

    pub(super) fn parse_abilities(
        &mut self,
        xmb: &Document,
        _filename: &str,
    ) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: u32 = 0;

        for node in &root.children {
            if node.name == "Ability" {
                let mut ability = Ability {
                    id: id_counter,
                    ..Default::default()
                };
                id_counter += 1;

                if let Some(name_attr) = node.get_attribute("name") {
                    ability.name = name_attr.value_string();
                }

                for child in &node.children {
                    match child.name.as_str() {
                        "Name" => ability.name = child.text_string(),
                        "DisplayNameID" => ability.display_name = child.text_string(),
                        "Type" => ability.ability_type = child.text_string(),
                        "RecoverTime" => {
                            ability.recover_time = child.text_string().parse().unwrap_or(0.0)
                        }
                        "MovementModifier" => {
                            ability.movement_modifier = child.text_string().parse().unwrap_or(1.0)
                        }
                        _ => {}
                    }
                }

                let id = ability.id;
                let name = ability.name.clone();
                self.abilities.insert(id, ability);
                self.abilities_by_name.insert(name, id);
            }
        }

        Ok(())
    }

    pub(super) fn parse_weapon_types(
        &mut self,
        xmb: &Document,
        _filename: &str,
    ) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: u32 = 0;

        for node in &root.children {
            if node.name == "WeaponType" {
                let mut weapon = WeaponType {
                    id: id_counter,
                    ..Default::default()
                };
                id_counter += 1;

                if let Some(name_attr) = node.get_attribute("name") {
                    weapon.name = name_attr.value_string();
                }

                // Engine (sub_1405C01A0) parses: Name, DeathAnimation, DamageModifier
                // DamageModifier has attributes: type, rating, reflectDamageFactor, bowlable, rammable
                for child in &node.children {
                    match child.name.as_str() {
                        "Name" => weapon.name = child.text_string(),
                        "DeathAnimation" => weapon.death_animation = child.text_string(),
                        "DamageModifier" => {
                            let mut dm = DamageModifier::default();
                            if let Some(t) = child.get_attribute("type") {
                                dm.damage_type = t.value_string();
                            }
                            if let Some(r) = child.get_attribute("rating") {
                                dm.rating = r.value_string().parse().unwrap_or(0);
                            }
                            dm.modifier = child.text_string().parse().unwrap_or(0.0);
                            if let Some(r) = child.get_attribute("reflectDamageFactor") {
                                dm.reflect_damage_factor = r.value_string().parse().unwrap_or(0);
                            }
                            if let Some(b) = child.get_attribute("bowlable") {
                                dm.bowlable = b.value_string().eq_ignore_ascii_case("true");
                            }
                            if let Some(r) = child.get_attribute("rammable") {
                                dm.rammable = r.value_string().eq_ignore_ascii_case("true");
                            }
                            weapon.damage_modifiers.push(dm);
                        }
                        _ => {}
                    }
                }

                let id = weapon.id;
                let name = weapon.name.clone();
                self.weapon_types.insert(id, weapon);
                self.weapon_types_by_name.insert(name, id);
            }
        }

        Ok(())
    }

    pub(super) fn parse_game_modes(
        &mut self,
        xmb: &Document,
        _filename: &str,
    ) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: u32 = 0;

        for node in &root.children {
            if node.name == "GameMode" {
                let mut mode = GameMode {
                    id: id_counter,
                    ..Default::default()
                };
                id_counter += 1;

                // Attributes: Locked, dlc
                if let Some(a) = node.get_attribute("Locked") {
                    mode.locked = a.value_string().eq_ignore_ascii_case("true");
                }
                if let Some(a) = node.get_attribute("dlc") {
                    mode.dlc = a.value_string().eq_ignore_ascii_case("true");
                }

                // Engine (sub_140240E70) parses these child tags:
                for child in &node.children {
                    match child.name.as_str() {
                        "Name" => mode.name = child.text_string(),
                        "WorldScript" => mode.world_script = child.text_string(),
                        "PlayerScript" => mode.player_script = child.text_string(),
                        "NPC" => mode.npc = child.text_string(),
                        "Tech" => {
                            // Engine does a tech string table lookup; we store raw for now
                            mode.tech_id = self.resolve_tech_id(&child.text_string());
                        }
                        "DisplayNameID" => {
                            mode.display_name_id = child.text_string().parse().unwrap_or(-1);
                        }
                        "DescriptionID" => {
                            mode.description_id = child.text_string().parse().unwrap_or(-1);
                        }
                        "LongDescriptionID" => {
                            mode.long_description_id = child.text_string().parse().unwrap_or(-1);
                        }
                        _ => {}
                    }
                }

                let id = mode.id;
                let name = mode.name.clone();
                self.game_modes.insert(id, mode);
                self.game_modes_by_name.insert(name, id);
            }
        }

        Ok(())
    }

    pub(super) fn parse_damage_types(
        &mut self,
        xmb: &Document,
        _filename: &str,
    ) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: u32 = 0;

        for node in &root.children {
            if node.name == "DamageType" {
                let mut dtype = DamageType {
                    id: id_counter,
                    ..Default::default()
                };
                id_counter += 1;

                if let Some(name_attr) = node.get_attribute("name") {
                    dtype.name = name_attr.value_string();
                }

                for child in &node.children {
                    let text = child.text_string();
                    match child.name.as_str() {
                        "Name" => dtype.name = text,
                        "Shielded" => dtype.shielded = text == "true" || text == "1",
                        "Attenuates" => dtype.attenuates = text == "true" || text == "1",
                        _ => {}
                    }
                }

                let id = dtype.id;
                let name = dtype.name.clone();
                self.damage_types.insert(id, dtype);
                self.damage_types_by_name.insert(name, id);
            }
        }

        Ok(())
    }

    pub(super) fn parse_game_data(
        &mut self,
        xmb: &Document,
        _filename: &str,
    ) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        for child in &root.children {
            let text = child.text_string();
            match child.name.as_str() {
                "Resources" => {
                    for res_child in &child.children {
                        match res_child.name.as_str() {
                            "Deductable" => {
                                for d in &res_child.children {
                                    let val = d.text_string() == "true" || d.text_string() == "1";
                                    match d.name.as_str() {
                                        "Supplies" => self.game_data.supplies_deductable = val,
                                        "Power" => self.game_data.power_deductable = val,
                                        "LeaderPowerCharge" => {
                                            self.game_data.leader_power_charge_deductable = val
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            "Rates" => {
                                for r in &res_child.children {
                                    let val: f32 = r.text_string().parse().unwrap_or(1.0);
                                    match r.name.as_str() {
                                        "Supplies" => self.game_data.supplies_rate = val,
                                        "Power" => self.game_data.power_rate = val,
                                        "LeaderPowerCharge" => {
                                            self.game_data.leader_power_charge_rate = val
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "Pops" => {
                    for pop in &child.children {
                        let name_str = pop
                            .get_attribute("name")
                            .map(|a| a.value_string())
                            .or_else(|| {
                                pop.children
                                    .iter()
                                    .find(|c| c.name == "Name")
                                    .map(|c| c.text_string())
                            })
                            .unwrap_or_default();
                        let max: i32 = pop
                            .get_attribute("max")
                            .and_then(|a| a.value_string().parse().ok())
                            .unwrap_or(0);
                        let pop_id = self.game_data.pops.len() as u32;
                        self.game_data.pops.push(PopDefinition {
                            id: pop_id,
                            name: name_str,
                            max,
                        });
                    }
                }
                "GarrisonDamageMultiplier" => {
                    self.game_data.garrison_damage_multiplier = text.parse().unwrap_or(1.0)
                }
                "ConstructionDamageMultiplier" => {
                    self.game_data.construction_damage_multiplier = text.parse().unwrap_or(1.0)
                }
                "CaptureDecayRate" => {
                    self.game_data.capture_decay_rate = text.parse().unwrap_or(0.0)
                }
                "ProjectileGravity" => {
                    self.game_data.projectile_gravity = text.parse().unwrap_or(9.81)
                }
                "ProjectileTumbleRate" => {
                    self.game_data.projectile_tumble_rate = text.parse().unwrap_or(0.0)
                }
                "HeightBonusDamage" => {
                    self.game_data.height_bonus_damage = text.parse().unwrap_or(0.0)
                }
                "AttackRatingMultiplier" => {
                    self.game_data.attack_rating_multiplier = text.parse().unwrap_or(1.0)
                }
                "DefenseRatingMultiplier" => {
                    self.game_data.defense_rating_multiplier = text.parse().unwrap_or(1.0)
                }
                "ChanceToRocket" => self.game_data.chance_to_rocket = text.parse().unwrap_or(0.0),
                "DamageBankTimer" => self.game_data.damage_bank_timer = text.parse().unwrap_or(0.0),
                "MaxDamageBankPctAdjust" => {
                    self.game_data.max_damage_bank_pct_adjust = text.parse().unwrap_or(0.0)
                }
                "SquadLeashLength" => {
                    self.game_data.squad_leash_length = text.parse().unwrap_or(0.0)
                }
                "SquadAggroLength" => {
                    self.game_data.squad_aggro_length = text.parse().unwrap_or(0.0)
                }
                "UnitLeashLength" => self.game_data.unit_leash_length = text.parse().unwrap_or(0.0),
                "ShieldRegenDelay" => {
                    self.game_data.shield_regen_delay = text.parse().unwrap_or(0.0)
                }
                "ShieldRegenTime" => self.game_data.shield_regen_time = text.parse().unwrap_or(0.0),
                "CloakingDelay" => self.game_data.cloaking_delay = text.parse().unwrap_or(0.0),
                "ReCloakDelay" => self.game_data.re_cloak_delay = text.parse().unwrap_or(0.0),
                "CloakDetectFrequency" => {
                    self.game_data.cloak_detect_frequency = text.parse().unwrap_or(0.0)
                }
                "HeroDownedLOS" => self.game_data.hero_downed_los = text.parse().unwrap_or(0.0),
                "HeroHPRegenTime" => {
                    self.game_data.hero_hp_regen_time = text.parse().unwrap_or(0.0)
                }
                "HeroRevivalDistance" => {
                    self.game_data.hero_revival_distance = text.parse().unwrap_or(0.0)
                }
                "HeroPercentHPRevivalThreshhold" => {
                    self.game_data.hero_percent_hp_revival_threshold = text.parse().unwrap_or(0.0)
                }
                "TransportMax" => self.game_data.transport_max = text.parse().unwrap_or(0),
                "TransportIncomingHeight" => {
                    self.game_data.transport_incoming_height = text.parse().unwrap_or(0.0)
                }
                "TransportOutgoingHeight" => {
                    self.game_data.transport_outgoing_height = text.parse().unwrap_or(0.0)
                }
                "TransportPickupHeight" => {
                    self.game_data.transport_pickup_height = text.parse().unwrap_or(0.0)
                }
                "TransportDropoffHeight" => {
                    self.game_data.transport_dropoff_height = text.parse().unwrap_or(0.0)
                }
                "OverrunDistance" => self.game_data.overrun_distance = text.parse().unwrap_or(0.0),
                "OverrunMinVel" => self.game_data.overrun_min_vel = text.parse().unwrap_or(0.0),
                "OverrunJumpForce" => {
                    self.game_data.overrun_jump_force = text.parse().unwrap_or(0.0)
                }
                "TributeAmount" => self.game_data.tribute_amount = text.parse().unwrap_or(0.0),
                "TributeCost" => self.game_data.tribute_cost = text.parse().unwrap_or(0.0),
                "UnscSupplyPadBonus" => {
                    self.game_data.unsc_supply_pad_bonus = text.parse().unwrap_or(0.0)
                }
                "UnscSupplyPadBreakEvenPoint" => {
                    self.game_data.unsc_supply_pad_break_even_point = text.parse().unwrap_or(0.0)
                }
                "CovSupplyPadBonus" => {
                    self.game_data.cov_supply_pad_bonus = text.parse().unwrap_or(0.0)
                }
                "CovSupplyPadBreakEvenPoint" => {
                    self.game_data.cov_supply_pad_break_even_point = text.parse().unwrap_or(0.0)
                }
                "LeaderPowerChargeResource" => {
                    self.game_data.leader_power_charge_resource = text.parse().unwrap_or(0)
                }
                "LeaderPowerChargeRate" => {
                    self.game_data.leader_power_charge_rate_value = text.parse().unwrap_or(0.0)
                }
                "RecyleRefundRate" => {
                    self.game_data.recycle_refund_rate = text.parse().unwrap_or(0.0)
                }
                "BaseRebuildTimer" => {
                    self.game_data.base_rebuild_timer = text.parse().unwrap_or(0.0)
                }
                "CoopResourceSplitRate" => {
                    self.game_data.coop_resource_split_rate = text.parse().unwrap_or(0.0)
                }
                "GameOverDelay" => self.game_data.game_over_delay = text.parse().unwrap_or(0.0),
                "MaxNumCorpses" => self.game_data.max_num_corpses = text.parse().unwrap_or(0),
                "BuildingSelfDestructTime" => {
                    self.game_data.building_self_destruct_time = text.parse().unwrap_or(0.0)
                }
                "DamageReceivedXPFactor" => {
                    self.game_data.damage_received_xp_factor = text.parse().unwrap_or(0.0)
                }
                "AirStrikeLoiterTime" => {
                    self.game_data.air_strike_loiter_time = text.parse().unwrap_or(0.0)
                }
                "DefaultCryoPoints" => {
                    self.game_data.default_cryo_points = text.parse().unwrap_or(0.0)
                }
                "DefaultThawSpeed" => {
                    self.game_data.default_thaw_speed = text.parse().unwrap_or(0.0)
                }
                "FrozenDamageModifier" => {
                    self.game_data.frozen_damage_modifier = text.parse().unwrap_or(1.0)
                }
                "FreezingSpeedModifier" => {
                    self.game_data.freezing_speed_modifier = text.parse().unwrap_or(1.0)
                }
                "FreezingDamageModifier" => {
                    self.game_data.freezing_damage_modifier = text.parse().unwrap_or(1.0)
                }
                "TimeFrozenToThaw" => {
                    self.game_data.time_frozen_to_thaw = text.parse().unwrap_or(0.0)
                }
                "TimeFreezingToThaw" => {
                    self.game_data.time_freezing_to_thaw = text.parse().unwrap_or(0.0)
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub(super) fn parse_object_types(
        &mut self,
        xmb: &Document,
        _filename: &str,
    ) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: u32 = self.object_types.len() as u32;

        for node in &root.children {
            if node.name == "ObjectType" {
                let name = node
                    .get_attribute("name")
                    .map(|a| a.value_string())
                    .or_else(|| {
                        node.children
                            .iter()
                            .find(|c| c.name == "Name")
                            .map(|c| c.text_string())
                    })
                    .unwrap_or_else(|| node.text_string());

                if !name.is_empty() {
                    let entry = ObjectTypeEntry {
                        id: id_counter,
                        name: name.clone(),
                    };
                    self.object_types.insert(id_counter, entry);
                    self.object_types_by_name.insert(name, id_counter);
                    id_counter += 1;
                }
            }
        }

        Ok(())
    }

    pub(super) fn parse_terrain_tile_types(
        &mut self,
        xmb: &Document,
        _filename: &str,
    ) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: u32 = self.terrain_tile_types.len() as u32;

        for node in &root.children {
            let name = node
                .get_attribute("name")
                .map(|a| a.value_string())
                .unwrap_or_else(|| node.text_string());

            if !name.is_empty() {
                let entry = TerrainTileType {
                    id: id_counter,
                    name: name.clone(),
                };
                self.terrain_tile_types.insert(id_counter, entry);
                self.terrain_tile_types_by_name.insert(name, id_counter);
                id_counter += 1;
            }
        }

        Ok(())
    }

    /// Parse player colors from playercolors.xml.
    /// Matches BDatabase__loadPlayerColors (0x1401F9560).
    ///
    /// Structure:
    ///   <PlayerColors>
    ///     <spc>                         — campaign color set
    ///       <color num="0-15" objects="..." corpse="..." selection="..." minimap="..." ui="..."/>
    ///       <friendOrFoeSelf objects="..." .../>
    ///       <friendOrFoeAlly .../>
    ///       <friendOrFoeNeutral .../>
    ///       <friendOrFoeEnemy .../>
    ///     </spc>
    ///     <skirmish>                    — skirmish color set (same children)
    ///       ...
    ///     </skirmish>
    ///     <Civ>CivName                  — per-civ overrides
    ///       <player num="0-8" color="0xAARRGGBB"/>
    ///     </Civ>
    ///   </PlayerColors>
    pub(super) fn parse_player_colors(
        &mut self,
        xmb: &Document,
        _filename: &str,
    ) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        for node in &root.children {
            let name = node.name.as_str();

            if name.eq_ignore_ascii_case("spc") || name.eq_ignore_ascii_case("skirmish") {
                let is_spc = name.eq_ignore_ascii_case("spc");
                let set = if is_spc {
                    &mut self.player_color_data.spc
                } else {
                    &mut self.player_color_data.skirmish
                };

                for child in &node.children {
                    let child_name = child.name.as_str();

                    if child_name.eq_ignore_ascii_case("color") {
                        // "color" child with "num" attribute (0-15)
                        let num = Self::parse_strtol_attr(child, "num").unwrap_or(0) as usize;
                        if num <= 15 {
                            Self::parse_player_color_attrs(child, &mut set.player_colors[num]);
                        }
                    } else if child_name.eq_ignore_ascii_case("friendOrFoeSelf") {
                        Self::parse_player_color_attrs(child, &mut set.friend_or_foe_self);
                    } else if child_name.eq_ignore_ascii_case("friendOrFoeAlly") {
                        Self::parse_player_color_attrs(child, &mut set.friend_or_foe_ally);
                    } else if child_name.eq_ignore_ascii_case("friendOrFoeNeutral") {
                        Self::parse_player_color_attrs(child, &mut set.friend_or_foe_neutral);
                    } else if child_name.eq_ignore_ascii_case("friendOrFoeEnemy") {
                        Self::parse_player_color_attrs(child, &mut set.friend_or_foe_enemy);
                    }
                }
            } else if name.eq_ignore_ascii_case("Civ") {
                // Per-civ color overrides
                let civ_name = node.text_string();
                let civ_id = self.resolve_civ_id(&civ_name);
                if civ_id < 0 || civ_id > 4 {
                    continue;
                }

                let mut civ_colors = CivPlayerColors {
                    civ_id,
                    ..Default::default()
                };

                for child in &node.children {
                    if child.name.eq_ignore_ascii_case("player") {
                        // "player" child with "num" (0-8) and "color" attributes
                        let num = Self::parse_strtol_attr(child, "num").unwrap_or(0) as usize;
                        if num > 8 {
                            continue;
                        }
                        if let Some(color) = Self::parse_strtol_attr(child, "color") {
                            if color <= 0xF {
                                // Engine stores this as a DWORD directly
                                civ_colors.colors[num] = color;
                            }
                        }
                    }
                }

                self.player_color_data.civ_colors.push(civ_colors);
            }
        }

        log::info!("Loaded player colors (spc + skirmish + {} civ overrides)",
            self.player_color_data.civ_colors.len());
        Ok(())
    }

    /// Parse BPlayerColor attributes ("objects", "corpse", "selection", "minimap", "ui")
    /// from a node. Each is a color string converted via sub_1406875D0 (string-to-DWORD).
    /// The engine calls BXMLReader__getAttributeAsString for each, then converts.
    fn parse_player_color_attrs(node: &crate::xmb::Node, out: &mut PlayerColor) {
        if let Some(v) = Self::parse_color_attr(node, "objects") {
            out.objects = v;
        }
        if let Some(v) = Self::parse_color_attr(node, "corpse") {
            out.corpse = v;
        }
        if let Some(v) = Self::parse_color_attr(node, "selection") {
            out.selection = v;
        }
        if let Some(v) = Self::parse_color_attr(node, "minimap") {
            out.minimap = v;
        }
        if let Some(v) = Self::parse_color_attr(node, "ui") {
            out.ui = v;
        }
    }

    /// Parse a color attribute value. The engine uses sub_1406875D0 which converts
    /// a named color string or hex value to a DWORD. We support hex (0xAARRGGBB)
    /// and decimal integer strings.
    fn parse_color_attr(node: &crate::xmb::Node, attr_name: &str) -> Option<i32> {
        let attr = node.get_attribute(attr_name)?;
        let text = attr.value_string();
        if text.is_empty() {
            return None;
        }
        Self::parse_color_string(&text)
    }

    /// Convert a color string to i32. Supports "0xAARRGGBB" hex and decimal.
    fn parse_color_string(s: &str) -> Option<i32> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            u32::from_str_radix(hex, 16).ok().map(|v| v as i32)
        } else {
            s.parse::<i32>().ok()
        }
    }

    /// Parse a strtol-style integer attribute (base 10).
    fn parse_strtol_attr(node: &crate::xmb::Node, attr_name: &str) -> Option<i32> {
        let attr = node.get_attribute(attr_name)?;
        let text = attr.value_string();
        text.trim().parse::<i32>().ok()
    }
}
