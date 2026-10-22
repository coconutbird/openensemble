//! Immutable prototype data used by retail squad-mode transitions.

use crate::entities::SquadMode;
use pipeline::database::hw1::Database;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default)]
pub(super) struct ChangeModeCatalog {
    animations: BTreeMap<(String, SquadMode), String>,
    lockdown_menus: BTreeSet<String>,
    ability_disabled: BTreeSet<String>,
    auto_lock: BTreeSet<String>,
    auto_unlock: BTreeSet<String>,
    vehicle_goto: BTreeSet<String>,
    rally_points: BTreeSet<String>,
}

impl ChangeModeCatalog {
    pub(super) fn from_database(database: &Database) -> Self {
        let mut catalog = Self::default();
        for object in &database.objects {
            let prototype = object.name.trim().to_ascii_lowercase();
            if object
                .flags
                .iter()
                .any(|flag| flag.eq_ignore_ascii_case("LockdownMenu"))
            {
                catalog.lockdown_menus.insert(prototype.clone());
            }
            if object
                .flags
                .iter()
                .any(|flag| flag.eq_ignore_ascii_case("AbilityDisabled"))
            {
                catalog.ability_disabled.insert(prototype.clone());
            }
            match object.auto_lock_down.as_deref().map(str::trim) {
                Some(kind) if kind.eq_ignore_ascii_case("LockAndUnlock") => {
                    catalog.auto_lock.insert(prototype.clone());
                    catalog.auto_unlock.insert(prototype.clone());
                }
                Some(kind) if kind.eq_ignore_ascii_case("LockOnly") => {
                    catalog.auto_lock.insert(prototype.clone());
                }
                Some(kind) if kind.eq_ignore_ascii_case("UnlockOnly") => {
                    catalog.auto_unlock.insert(prototype.clone());
                }
                _ => {}
            }
            if object
                .goto_type
                .as_deref()
                .is_some_and(|kind| kind.eq_ignore_ascii_case("Vehicle"))
            {
                catalog.vehicle_goto.insert(prototype.clone());
            }
            if object.rally_point.as_deref().is_some_and(|kind| {
                kind.eq_ignore_ascii_case("Military") || kind.eq_ignore_ascii_case("Civilian")
            }) {
                catalog.rally_points.insert(prototype.clone());
            }
            for animation in &object.squad_mode_animations {
                let Some(mode) = animation.mode.as_deref().and_then(SquadMode::from_authored)
                else {
                    continue;
                };
                let animation_type = animation.animation.trim();
                if !prototype.is_empty() && !animation_type.is_empty() {
                    catalog
                        .animations
                        .insert((prototype.clone(), mode), animation_type.to_owned());
                }
            }
        }
        catalog
    }

    pub(super) fn animation(&self, prototype: &str, mode: SquadMode) -> Option<&str> {
        self.animations
            .get(&(prototype.trim().to_ascii_lowercase(), mode))
            .map(String::as_str)
    }

    pub(super) fn has_lockdown_menu(&self, prototype: &str) -> bool {
        self.lockdown_menus
            .contains(&prototype.trim().to_ascii_lowercase())
    }

    pub(super) fn ability_starts_disabled(&self, prototype: &str) -> bool {
        self.ability_disabled
            .contains(&prototype.trim().to_ascii_lowercase())
    }

    pub(super) fn can_auto_lock(&self, prototype: &str) -> bool {
        self.auto_lock
            .contains(&prototype.trim().to_ascii_lowercase())
    }

    pub(super) fn can_auto_unlock(&self, prototype: &str) -> bool {
        self.auto_unlock
            .contains(&prototype.trim().to_ascii_lowercase())
    }

    pub(super) fn uses_vehicle_goto(&self, prototype: &str) -> bool {
        self.vehicle_goto
            .contains(&prototype.trim().to_ascii_lowercase())
    }

    pub(super) fn has_rally_point(&self, prototype: &str) -> bool {
        self.rally_points
            .contains(&prototype.trim().to_ascii_lowercase())
    }
}

impl super::GameplayCatalog {
    /// Resolve the unit animation authored for entering one squad mode.
    #[must_use]
    pub fn squad_mode_animation(&self, prototype: &str, mode: SquadMode) -> Option<&str> {
        self.change_modes.animation(prototype, mode)
    }

    /// Return whether the prototype exposes retail's deployed building menu.
    #[must_use]
    pub fn has_lockdown_menu(&self, prototype: &str) -> bool {
        self.change_modes.has_lockdown_menu(prototype)
    }

    pub(crate) fn change_mode_ability_starts_disabled(&self, prototype: &str) -> bool {
        self.change_modes.ability_starts_disabled(prototype)
    }

    pub(crate) fn can_auto_lock(&self, prototype: &str) -> bool {
        self.change_modes.can_auto_lock(prototype)
    }

    pub(crate) fn can_auto_unlock(&self, prototype: &str) -> bool {
        self.change_modes.can_auto_unlock(prototype)
    }

    pub(crate) fn uses_vehicle_goto(&self, prototype: &str) -> bool {
        self.change_modes.uses_vehicle_goto(prototype)
    }

    pub(crate) fn has_rally_point(&self, prototype: &str) -> bool {
        self.change_modes.has_rally_point(prototype)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::ProtoObject;
    use pipeline::database::hw1::objects::SquadModeAnimation;

    #[test]
    fn mode_animation_and_lockdown_menu_are_case_insensitive() {
        let database = Database {
            objects: vec![ProtoObject {
                name: "Cobra".to_owned(),
                flags: vec!["LockdownMenu".to_owned()],
                squad_mode_animations: vec![SquadModeAnimation {
                    animation: "AnchorDown".to_owned(),
                    mode: Some("Lockdown".to_owned()),
                }],
                ..ProtoObject::default()
            }],
            ..Database::default()
        };
        let catalog = ChangeModeCatalog::from_database(&database);

        assert_eq!(
            catalog.animation("cobra", SquadMode::Lockdown),
            Some("AnchorDown")
        );
        assert!(catalog.has_lockdown_menu("COBRA"));
        assert_eq!(catalog.animation("cobra", SquadMode::Normal), None);
    }

    #[test]
    fn automatic_mode_and_vehicle_rally_metadata_are_preserved() {
        let database = Database {
            objects: vec![ProtoObject {
                name: "Elephant".to_owned(),
                auto_lock_down: Some("LockAndUnlock".to_owned()),
                goto_type: Some("Vehicle".to_owned()),
                rally_point: Some("Military".to_owned()),
                ..ProtoObject::default()
            }],
            ..Database::default()
        };
        let catalog = ChangeModeCatalog::from_database(&database);

        assert!(catalog.can_auto_lock("elephant"));
        assert!(catalog.can_auto_unlock("ELEPHANT"));
        assert!(catalog.uses_vehicle_goto("elephant"));
        assert!(catalog.has_rally_point("elephant"));
    }
}
