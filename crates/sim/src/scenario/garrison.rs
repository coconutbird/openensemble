//! Scenario database projection for unit flags and containment capabilities.

use crate::entities::Unit;
use num_traits::ToPrimitive;
use pipeline::database::hw1::ProtoObject;

pub(super) fn configure_unit(unit: &mut Unit, proto: &ProtoObject) {
    unit.object_types = normalized_values(&proto.object_types);
    unit.flying = proto
        .movement_type
        .as_deref()
        .is_some_and(|movement| movement.eq_ignore_ascii_case("Air"));

    let teleporter = has_flag(proto, "Teleporter")
        || has_object_type(proto, "TeleportPickup")
        || has_object_type(proto, "HotDropPickup");
    let one_squad_containment = has_flag(proto, "OneSquadContainment");
    let can_contain = teleporter || proto.max_contained.is_some() || !proto.contain.is_empty();
    if !can_contain {
        return;
    }

    let maximum_population = proto
        .max_contained
        .and_then(|value| value.to_f32())
        .unwrap_or_default();
    unit.garrison.configure_container(
        maximum_population,
        one_squad_containment,
        teleporter,
        normalized_values(&proto.contain),
    );
}

fn has_flag(proto: &ProtoObject, expected: &str) -> bool {
    proto
        .flags
        .iter()
        .any(|flag| flag.trim().eq_ignore_ascii_case(expected))
}

fn has_object_type(proto: &ProtoObject, expected: &str) -> bool {
    proto
        .object_types
        .iter()
        .any(|object_type| object_type.trim().eq_ignore_ascii_case(expected))
}

fn normalized_values(values: &[String]) -> Vec<String> {
    let mut values = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    values.sort_by_key(|value| value.to_ascii_lowercase());
    values.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    values
}
