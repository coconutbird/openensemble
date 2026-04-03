//! Parsers for proto objects, squads, and techs.

use super::DatabaseError;
use super::GameDatabase;
use crate::proto::{
    ObjectType, ProtoFlags, ProtoId, ProtoObject, ProtoSquad, ProtoTech, ResourceCost, SquadUnit,
    TechEffect,
};
use crate::xmb::{Document, Node};

impl GameDatabase {
    pub(super) fn parse_objects(
        &mut self,
        xmb: &Document,
        _filename: &str,
    ) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: ProtoId = 0;

        for node in &root.children {
            if node.name == "Object" {
                let obj = Self::parse_proto_object(node, &mut id_counter);
                self.proto.add_object(obj);
            }
        }

        Ok(())
    }

    fn parse_proto_object(node: &Node, id_counter: &mut ProtoId) -> ProtoObject {
        let mut obj = ProtoObject {
            id: *id_counter,
            ..Default::default()
        };
        *id_counter += 1;

        if let Some(name_attr) = node.get_attribute("name") {
            obj.name = name_attr.value_string();
        }

        for child in &node.children {
            match child.name.as_str() {
                "Name" => obj.name = child.text_string(),
                "DisplayNameID" => obj.display_name = child.text_string(),
                "Hitpoints" => obj.hitpoints = child.text_string().parse().unwrap_or(0.0),
                "ShieldPoints" => obj.shield_points = child.text_string().parse().unwrap_or(0.0),
                "MaxVelocity" => obj.movement_speed = child.text_string().parse().unwrap_or(0.0),
                "BuildPoints" => obj.build_time = child.text_string().parse().unwrap_or(0.0),
                "Cost" => Self::parse_cost(child, &mut obj.cost),
                "PopCap" => obj.population_cost = child.text_string().parse().unwrap_or(0),
                "PopMax" => obj.population_capacity = child.text_string().parse().unwrap_or(0),
                "ObjectClass" => obj.object_type = Self::parse_object_class(&child.text_string()),
                "Flag" => Self::parse_flag(child, &mut obj.flags),
                _ => {}
            }
        }

        obj
    }

    pub(super) fn parse_cost(node: &Node, cost: &mut ResourceCost) {
        for attr in &node.attributes {
            match attr.name.as_str() {
                "Supplies" | "supplies" => {
                    cost.supplies = attr.value_string().parse().unwrap_or(0.0)
                }
                "Power" | "power" => cost.power = attr.value_string().parse().unwrap_or(0.0),
                "Pop" | "pop" | "Population" => {
                    cost.population = attr.value_string().parse().unwrap_or(0)
                }
                _ => {}
            }
        }
        // Also check text content for simple cost values
        let text = node.text_string();
        if !text.is_empty()
            && let Ok(v) = text.parse::<f32>()
        {
            cost.supplies = v;
        }
    }

    fn parse_object_class(class: &str) -> ObjectType {
        match class.to_lowercase().as_str() {
            "unit" | "infantry" | "vehicle" | "aircraft" => ObjectType::Unit,
            "building" | "structure" => ObjectType::Building,
            "projectile" => ObjectType::Projectile,
            "effect" => ObjectType::Effect,
            "resource" | "supplycrate" => ObjectType::Resource,
            _ => ObjectType::Unit,
        }
    }

    fn parse_flag(node: &Node, flags: &mut ProtoFlags) {
        let flag_name = node.text_string();
        match flag_name.to_lowercase().as_str() {
            "canattack" | "attackable" => flags.can_attack = true,
            "mobile" | "canmove" => flags.can_move = true,
            "flying" | "flyer" => flags.flying = true,
            "infantry" => flags.infantry = true,
            "vehicle" => flags.vehicle = true,
            "aircraft" => flags.aircraft = true,
            "building" | "isbuilding" => flags.building = true,
            "hero" => flags.hero = true,
            _ => {}
        }
    }

    pub(super) fn parse_squads(
        &mut self,
        xmb: &Document,
        _filename: &str,
    ) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: ProtoId = 0;

        for node in &root.children {
            if node.name == "Squad" {
                let squad = Self::parse_proto_squad(node, &mut id_counter);
                self.proto.add_squad(squad);
            }
        }

        Ok(())
    }

    fn parse_proto_squad(node: &Node, id_counter: &mut ProtoId) -> ProtoSquad {
        let mut squad = ProtoSquad {
            id: *id_counter,
            ..Default::default()
        };
        *id_counter += 1;

        if let Some(name_attr) = node.get_attribute("name") {
            squad.name = name_attr.value_string();
        }

        for child in &node.children {
            match child.name.as_str() {
                "Name" => squad.name = child.text_string(),
                "FormationType" => squad.formation = child.text_string(),
                "Unit" => {
                    let mut unit = SquadUnit::default();
                    if let Some(count) = child.get_attribute("count") {
                        unit.count = count.value_string().parse().unwrap_or(1);
                    }
                    let _proto_name = child.text_string();
                    unit.proto_id = 0;
                    squad.units.push(unit);
                }
                _ => {}
            }
        }

        squad
    }

    pub(super) fn parse_techs(
        &mut self,
        xmb: &Document,
        _filename: &str,
    ) -> Result<(), DatabaseError> {
        let root = match xmb.root() {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut id_counter: ProtoId = 0;

        for node in &root.children {
            if node.name == "Tech" {
                let tech = Self::parse_proto_tech(node, &mut id_counter);
                self.proto.add_tech(tech);
            }
        }

        Ok(())
    }

    fn parse_proto_tech(node: &Node, id_counter: &mut ProtoId) -> ProtoTech {
        let mut tech = ProtoTech {
            id: *id_counter,
            ..Default::default()
        };
        *id_counter += 1;

        if let Some(name_attr) = node.get_attribute("name") {
            tech.name = name_attr.value_string();
        }

        for child in &node.children {
            match child.name.as_str() {
                "Name" => tech.name = child.text_string(),
                "DisplayNameID" => tech.display_name = child.text_string(),
                "ResearchPoints" => tech.research_time = child.text_string().parse().unwrap_or(0.0),
                "Cost" => Self::parse_cost(child, &mut tech.cost),
                "Effect" => {
                    let effect = Self::parse_tech_effect(child);
                    tech.effects.push(effect);
                }
                _ => {}
            }
        }

        tech
    }

    fn parse_tech_effect(node: &Node) -> TechEffect {
        let effect_type = node
            .get_attribute("type")
            .map(|a| a.value_string())
            .unwrap_or_default();
        let target = node
            .get_attribute("target")
            .map(|a| a.value_string())
            .unwrap_or_default();
        let amount: f32 = node
            .get_attribute("amount")
            .and_then(|a| a.value_string().parse().ok())
            .unwrap_or(0.0);

        match effect_type.to_lowercase().as_str() {
            "modifypercent" | "percent" => TechEffect::ModifyPercent {
                stat: target,
                amount,
            },
            "modifyabsolute" | "absolute" | "modify" => TechEffect::ModifyAbsolute {
                stat: target,
                amount,
            },
            "enableability" | "ability" => TechEffect::EnableAbility { ability: target },
            "unlock" => TechEffect::Unlock { proto_name: target },
            _ => TechEffect::ModifyPercent {
                stat: target,
                amount,
            },
        }
    }
}
