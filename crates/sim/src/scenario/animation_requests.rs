//! Animation clips referenced by scenario-layered gameplay database entries.

use pipeline::database::hw1::squads::Birth;
use pipeline::database::hw1::{Database, Squad};

pub(super) fn trained_birth_animation_requests(database: &Database) -> Vec<(String, String)> {
    let mut requests = Vec::new();
    for squad in &database.squads {
        let Some(birth) = squad.birth.as_ref() else {
            continue;
        };
        add_member_requests(&mut requests, squad, birth);
    }
    for trainer in &database.objects {
        for command in &trainer.commands {
            if !command
                .command_type
                .as_deref()
                .is_some_and(|kind| kind.eq_ignore_ascii_case("TrainSquad"))
            {
                continue;
            }
            let Some(animation) = find_squad(database, &command.target)
                .and_then(|squad| squad.birth.as_ref())
                .and_then(|birth| birth.trainer_animation.as_deref())
                .map(str::trim)
                .filter(|animation| !animation.is_empty())
            else {
                continue;
            };
            requests.push((trainer.name.clone(), animation.to_owned()));
        }
    }
    requests
}

fn add_member_requests(requests: &mut Vec<(String, String)>, squad: &Squad, birth: &Birth) {
    let animations = [
        birth.animation_0.as_deref(),
        birth.animation_1.as_deref(),
        birth.animation_2.as_deref(),
        birth.animation_3.as_deref(),
    ];
    let Some(units) = squad.units.as_ref() else {
        return;
    };
    for unit in &units.entries {
        for animation in animations
            .iter()
            .flatten()
            .map(|animation| animation.trim())
            .filter(|animation| !animation.is_empty())
        {
            requests.push((unit.proto_object.trim().to_owned(), animation.to_owned()));
        }
    }
}

fn find_squad<'database>(database: &'database Database, name: &str) -> Option<&'database Squad> {
    database
        .squads
        .iter()
        .find(|squad| squad.name.eq_ignore_ascii_case(name.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::ProtoObject;
    use pipeline::database::hw1::objects::ObjectCommand;
    use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
    use std::collections::BTreeSet;

    #[test]
    fn birth_requests_include_members_and_each_training_building() {
        let database = Database {
            objects: vec![trainer("barracks"), trainer("firebase")],
            squads: vec![Squad {
                name: "marine_squad".to_owned(),
                units: Some(UnitsWrapper {
                    entries: vec![UnitEntry {
                        proto_object: "marine".to_owned(),
                        count: 4,
                        ..UnitEntry::default()
                    }],
                }),
                birth: Some(Birth {
                    animation_0: Some("Birth0".to_owned()),
                    animation_1: Some("Birth1".to_owned()),
                    trainer_animation: Some("Train".to_owned()),
                    ..Birth::default()
                }),
                ..Squad::default()
            }],
            ..Database::default()
        };

        let requests = trained_birth_animation_requests(&database)
            .into_iter()
            .collect::<BTreeSet<_>>();

        assert_eq!(
            requests,
            BTreeSet::from([
                ("barracks".to_owned(), "Train".to_owned()),
                ("firebase".to_owned(), "Train".to_owned()),
                ("marine".to_owned(), "Birth0".to_owned()),
                ("marine".to_owned(), "Birth1".to_owned()),
            ])
        );
    }

    fn trainer(name: &str) -> ProtoObject {
        ProtoObject {
            name: name.to_owned(),
            commands: vec![ObjectCommand {
                target: "MARINE_SQUAD".to_owned(),
                command_type: Some("TrainSquad".to_owned()),
                ..ObjectCommand::default()
            }],
            ..ProtoObject::default()
        }
    }
}
