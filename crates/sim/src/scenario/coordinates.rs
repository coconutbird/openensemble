//! Scenario coordinate conventions owned by the simulation ingestion layer.

use super::{ScenarioData, ScenarioObject, scenario_object_position_to_world};

/// Horizontal axis convention used by scenario `<Position>` records.
///
/// Persistent `<Object>` records always use the XTD storage transpose. Player
/// positions vary between maps, so nearby transposed `sys_unitstart` object
/// markers provide map-local evidence for their convention.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScenarioPositionAxes {
    /// Position X/Z values are already expressed in terrain-world order.
    #[default]
    AuthoredWorld,
    /// Position X/Z values use the diagonally transposed terrain storage order.
    Transposed,
}

impl ScenarioPositionAxes {
    /// Infer the player-position convention from unit-start object markers.
    #[must_use]
    pub fn infer(scenario: &ScenarioData, max_players: Option<u32>) -> Self {
        let mut authored_score = 0.0;
        let mut transposed_score = 0.0;
        let mut evidence_count = 0_u32;

        for start in scenario
            .positions()
            .iter()
            .filter(|start| is_player_start(start.number, max_players))
        {
            let authored = Self::AuthoredWorld.position_to_world(start.position_vec3());
            let transposed = Self::Transposed.position_to_world(start.position_vec3());
            let Some(authored_distance) = nearest_unit_start_distance_squared(scenario, authored)
            else {
                continue;
            };
            let Some(transposed_distance) =
                nearest_unit_start_distance_squared(scenario, transposed)
            else {
                continue;
            };
            authored_score += authored_distance;
            transposed_score += transposed_distance;
            evidence_count += 1;
        }

        if evidence_count > 0 && transposed_score < authored_score {
            Self::Transposed
        } else {
            Self::AuthoredWorld
        }
    }

    /// Convert a scenario position into terrain-world coordinates.
    #[must_use]
    pub const fn position_to_world(self, position: [f32; 3]) -> [f32; 3] {
        match self {
            Self::AuthoredWorld => position,
            Self::Transposed => [position[2], position[1], position[0]],
        }
    }

    /// Convert a scenario direction into terrain-world axes.
    #[must_use]
    pub const fn direction_to_world(self, direction: [f32; 3]) -> [f32; 3] {
        self.position_to_world(direction)
    }
}

fn is_player_start(number: i32, max_players: Option<u32>) -> bool {
    u32::try_from(number)
        .is_ok_and(|number| number > 0 && max_players.is_none_or(|maximum| number <= maximum))
}

fn nearest_unit_start_distance_squared(
    scenario: &ScenarioData,
    player_position: [f32; 3],
) -> Option<f32> {
    scenario
        .objects()
        .iter()
        .filter_map(scenario_proto_name)
        .filter(|(_, name)| name.to_ascii_lowercase().starts_with("sys_unitstart"))
        .map(|(object, _)| {
            let position = scenario_object_position_to_world(object.position_vec3());
            let delta_x = position[0] - player_position[0];
            let delta_z = position[2] - player_position[2];
            delta_x.mul_add(delta_x, delta_z * delta_z)
        })
        .filter(|distance| distance.is_finite())
        .min_by(f32::total_cmp)
}

fn scenario_proto_name(object: &ScenarioObject) -> Option<(&ScenarioObject, &str)> {
    let direct = object.proto_name.trim();
    if !direct.is_empty() {
        return Some((object, direct));
    }

    let suffix = format!("_{}", object.id);
    object
        .editor_name
        .strip_suffix(&suffix)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| (object, name))
}

#[cfg(test)]
mod tests {
    use pipeline::hw1::scenario::{
        ObjectsWrapper, PositionsWrapper, ScenarioData, ScenarioObject, ScenarioPosition,
    };

    use super::ScenarioPositionAxes;

    #[test]
    fn unit_start_evidence_transposes_tundra_player_positions() {
        let scenario = scenario_with_axis_evidence(
            &[
                ("181.5322,-1.0026,214.2044", "sys_unitstart_01"),
                ("710.5275,-1.0232,682.3585", "sys_unitstart_01"),
            ],
            &["114.7806,-0.4714,222.1855", "777.4794,-0.6362,668.1328"],
        );

        assert_eq!(
            ScenarioPositionAxes::infer(&scenario, Some(2)),
            ScenarioPositionAxes::Transposed
        );
    }

    #[test]
    fn direct_player_positions_remain_direct_when_marker_evidence_matches() {
        let scenario =
            scenario_with_axis_evidence(&[("20,0,80", "sys_unitstart_01")], &["80,0,20"]);

        assert_eq!(
            ScenarioPositionAxes::infer(&scenario, Some(1)),
            ScenarioPositionAxes::AuthoredWorld
        );
    }

    #[test]
    fn scenarios_without_marker_evidence_keep_direct_player_positions() {
        assert_eq!(
            ScenarioPositionAxes::infer(&ScenarioData::default(), None),
            ScenarioPositionAxes::AuthoredWorld
        );
    }

    fn scenario_with_axis_evidence(objects: &[(&str, &str)], positions: &[&str]) -> ScenarioData {
        ScenarioData {
            objects: Some(ObjectsWrapper {
                entries: objects
                    .iter()
                    .enumerate()
                    .map(|(index, (position, proto_name))| ScenarioObject {
                        id: i32::try_from(index).expect("test object index fits i32"),
                        proto_name: (*proto_name).to_owned(),
                        position: (*position).to_owned(),
                        ..ScenarioObject::default()
                    })
                    .collect(),
            }),
            positions: Some(PositionsWrapper {
                entries: positions
                    .iter()
                    .enumerate()
                    .map(|(index, position)| ScenarioPosition {
                        number: i32::try_from(index + 1).expect("test position index fits i32"),
                        position: (*position).to_owned(),
                        ..ScenarioPosition::default()
                    })
                    .collect(),
            }),
            ..ScenarioData::default()
        }
    }
}
