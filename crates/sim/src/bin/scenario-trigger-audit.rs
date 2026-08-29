//! Print the trigger DBIDs authored into one or more installed-game scenarios.

use std::collections::BTreeSet;
use std::env;
use std::process::ExitCode;

use sim::load_scenario_from_game_dir;

fn main() -> ExitCode {
    let mut arguments = env::args().skip(1);
    let Some(game_dir) = arguments.next() else {
        eprintln!("usage: scenario-trigger-audit <game-dir> <scenario>...");
        return ExitCode::FAILURE;
    };
    let scenarios = arguments.collect::<Vec<_>>();
    if scenarios.is_empty() {
        eprintln!("at least one scenario name is required");
        return ExitCode::FAILURE;
    }
    let selected_dbids = selected_dbids();

    let mut failed = false;
    for scenario in scenarios {
        match load_scenario_from_game_dir(&game_dir, &scenario) {
            Ok(mut loaded) => {
                if env::var_os("OPENENSEMBLE_AUDIT_OBJECTIVES").is_some() {
                    print_scenario_objectives(&scenario, &mut loaded);
                }
                print_catalog(&scenario, loaded.simulation.world.trigger_engine());
                print_selected_bindings(
                    &scenario,
                    loaded.simulation.world.trigger_engine(),
                    &selected_dbids,
                );
                if env::var_os("OPENENSEMBLE_AUDIT_RAW_VARIABLES").is_some() {
                    print_raw_selected_variables(&scenario, &mut loaded, &selected_dbids);
                }
                print_selected_scenario_objects(&scenario, &loaded);
                if env::var_os("OPENENSEMBLE_AUDIT_ENTITIES").is_some() {
                    print_selected_entities(
                        &scenario,
                        &loaded.simulation.world,
                        &loaded.simulation.gameplay,
                        &selected_dbids,
                    );
                }
                let update = loaded.simulation.world.update_triggers_with_gameplay(
                    &loaded.content.database,
                    &loaded.simulation.gameplay,
                );
                println!("{scenario}: initial-update={update:?}");
                print_reached_selected_bindings(
                    &scenario,
                    loaded.simulation.world.trigger_engine(),
                    &selected_dbids,
                );
                if env::var_os("OPENENSEMBLE_AUDIT_PRESENTATION").is_some() {
                    print_presentation_state(&scenario, &loaded.simulation.world);
                }
                if update.infinite_loop_guard_reached {
                    print_hot_triggers(&scenario, loaded.simulation.world.trigger_engine());
                }
            }
            Err(error) => {
                failed = true;
                eprintln!("{scenario}: {error}");
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn print_selected_scenario_objects(scenario: &str, loaded: &sim::LoadedGameScenario) {
    let selected = env::var("OPENENSEMBLE_AUDIT_SCENARIO_IDS")
        .ok()
        .into_iter()
        .flat_map(|value| {
            value
                .split(',')
                .filter_map(|token| token.trim().parse::<i32>().ok())
                .collect::<Vec<_>>()
        })
        .collect::<BTreeSet<_>>();
    if selected.is_empty() {
        return;
    }
    let objects = loaded
        .content
        .scenario_data
        .as_ref()
        .and_then(|data| data.objects.as_ref())
        .map_or(&[][..], |objects| objects.entries.as_slice());
    for id in selected {
        let object = objects.iter().find(|object| object.id == id);
        let prototype = object.and_then(|object| {
            loaded
                .content
                .database
                .objects
                .iter()
                .find(|prototype| prototype.name.eq_ignore_ascii_case(&object.proto_name))
        });
        println!(
            "{scenario}: scenario-object id={id} source={object:?} prototype={prototype:?} sim={:?}",
            loaded.simulation.get_entity_id(id)
        );
    }
}

fn print_raw_selected_variables(
    scenario: &str,
    loaded: &mut sim::LoadedGameScenario,
    dbids: &BTreeSet<u16>,
) {
    let variable_ids = loaded
        .simulation
        .world
        .trigger_engine()
        .scripts()
        .flat_map(|(_, script)| {
            script.triggers.iter().flat_map(|trigger| {
                trigger
                    .effects_on_true
                    .iter()
                    .chain(&trigger.effects_on_false)
                    .filter(|effect| dbids.contains(&effect.raw_type))
                    .flat_map(|effect| effect.inputs.iter().chain(&effect.outputs))
                    .map(|binding| binding.variable_id)
            })
        })
        .collect::<BTreeSet<_>>();
    let Some(path) = loaded
        .content
        .scenario
        .as_ref()
        .map(pipeline::hw1::scenario::ScenarioDescriptor::scn_path)
    else {
        return;
    };
    let Some(document) = loaded.source.read_xmb(&path) else {
        return;
    };
    let Some(root) = document.root() else {
        return;
    };
    print_raw_variable_nodes(scenario, root, &variable_ids);
}

fn print_raw_variable_nodes(
    scenario: &str,
    node: &pipeline::xmb::Node,
    variable_ids: &BTreeSet<u32>,
) {
    if node.name == "TriggerVar"
        && let Some(id) = node
            .get_attribute("ID")
            .and_then(|attribute| attribute.value_string().parse::<u32>().ok())
        && variable_ids.contains(&id)
    {
        let attributes = node
            .attributes
            .iter()
            .map(|attribute| (attribute.name.as_str(), attribute.value_string()))
            .collect::<Vec<_>>();
        println!(
            "{scenario}: raw-variable id={id} attributes={attributes:?} text={:?}",
            node.text_string()
        );
    }
    for child in &node.children {
        print_raw_variable_nodes(scenario, child, variable_ids);
    }
}

fn print_scenario_objectives(scenario: &str, loaded: &mut sim::LoadedGameScenario) {
    let Some(path) = loaded
        .content
        .scenario
        .as_ref()
        .map(pipeline::hw1::scenario::ScenarioDescriptor::scn_path)
    else {
        return;
    };
    let Some(document) = loaded.source.read_xmb(&path) else {
        return;
    };
    let Some(objectives) = document
        .root()
        .and_then(|root| root.children.iter().find(|node| node.name == "Objectives"))
    else {
        return;
    };
    for objective in objectives
        .children
        .iter()
        .filter(|node| node.name == "Objective")
    {
        let attributes = objective
            .attributes
            .iter()
            .map(|attribute| (attribute.name.as_str(), attribute.value_string()))
            .collect::<Vec<_>>();
        let fields = objective
            .children
            .iter()
            .map(|field| (field.name.as_str(), field.text_string()))
            .collect::<Vec<_>>();
        println!(
            "{scenario}: objective attributes={attributes:?} text={:?} fields={fields:?}",
            objective.text_string(),
        );
    }
}

fn selected_dbids() -> BTreeSet<u16> {
    env::var("OPENENSEMBLE_AUDIT_DBIDS")
        .ok()
        .into_iter()
        .flat_map(|value| {
            value
                .split(',')
                .filter_map(|token| token.trim().parse().ok())
                .collect::<Vec<_>>()
        })
        .collect()
}

fn print_selected_bindings(scenario: &str, engine: &sim::TriggerEngine, dbids: &BTreeSet<u16>) {
    if dbids.is_empty() {
        return;
    }
    for (script_id, script) in engine.scripts() {
        for trigger in &script.triggers {
            for condition in trigger
                .conditions
                .iter()
                .filter(|condition| dbids.contains(&condition.raw_type))
            {
                println!(
                    "{scenario}: selected script={script_id} trigger={} name={:?} {}",
                    trigger.id,
                    trigger.name,
                    describe_condition(condition, script)
                );
            }
            for effect in trigger
                .effects_on_true
                .iter()
                .chain(&trigger.effects_on_false)
                .filter(|effect| dbids.contains(&effect.raw_type))
            {
                println!(
                    "{scenario}: selected script={script_id} trigger={} name={:?} {}",
                    trigger.id,
                    trigger.name,
                    describe_effect(effect, script)
                );
            }
        }
    }
}

fn print_reached_selected_bindings(
    scenario: &str,
    engine: &sim::TriggerEngine,
    dbids: &BTreeSet<u16>,
) {
    if dbids.is_empty() {
        return;
    }
    for (script_id, script) in engine.scripts() {
        for trigger in trigger_reached_selected(script, dbids) {
            for effect in trigger
                .effects_on_true
                .iter()
                .chain(&trigger.effects_on_false)
                .filter(|effect| dbids.contains(&effect.raw_type))
            {
                println!(
                    "{scenario}: reached-selected script={script_id} trigger={} evaluations={} name={:?} {}",
                    trigger.id,
                    trigger.evaluate_count,
                    trigger.name,
                    describe_effect(effect, script)
                );
            }
        }
    }
}

fn print_selected_entities(
    scenario: &str,
    world: &sim::World,
    gameplay: &sim::GameplayCatalog,
    dbids: &BTreeSet<u16>,
) {
    let mut entity_ids = Vec::new();
    for (_, script) in world.trigger_engine().scripts() {
        for trigger in &script.triggers {
            for effect in trigger
                .effects_on_true
                .iter()
                .chain(&trigger.effects_on_false)
                .filter(|effect| dbids.contains(&effect.raw_type))
            {
                for binding in effect.inputs.iter().chain(&effect.outputs) {
                    let Some(entity_id) = script
                        .get_variable(binding.variable_id)
                        .and_then(|variable| variable.value.as_entity())
                    else {
                        continue;
                    };
                    if !entity_ids.contains(&entity_id) {
                        entity_ids.push(entity_id);
                    }
                }
            }
        }
    }
    entity_ids.sort_unstable();
    for entity_id in entity_ids {
        let Some(squad) = world.get_squad(entity_id) else {
            println!("{scenario}: selected-entity id={entity_id:?} missing-squad");
            continue;
        };
        println!(
            "{scenario}: selected-entity id={entity_id:?} player={} proto-squad={:?} position={:?} forward={:?} units={:?}",
            squad.base.player_id,
            squad.proto_squad_name,
            squad.base.position,
            squad.base.forward,
            squad.unit_ids,
        );
        for &unit_id in &squad.unit_ids {
            let Some(unit) = world.get_unit(unit_id) else {
                continue;
            };
            let tower_actions = gameplay
                .object(&unit.proto_object_name)
                .into_iter()
                .flat_map(|object| {
                    object
                        .tactics()
                        .actions
                        .iter()
                        .filter(|action| {
                            action
                                .action_type
                                .as_deref()
                                .is_some_and(|kind| kind.eq_ignore_ascii_case("TowerWall"))
                        })
                        .map(|action| {
                            let weapon =
                                action.weapon.as_deref().and_then(|weapon_name| {
                                    object.tactics().weapons.iter().find(|weapon| {
                                        weapon.name.eq_ignore_ascii_case(weapon_name)
                                    })
                                });
                            (
                                &action.name,
                                &action.weapon,
                                weapon.and_then(|weapon| weapon.projectile.as_deref()),
                                weapon
                                    .and_then(|weapon| weapon.projectile.as_deref())
                                    .and_then(|projectile| gameplay.projectile(projectile)),
                                &action.beam,
                            )
                        })
                })
                .collect::<Vec<_>>();
            println!(
                "{scenario}: selected-unit id={unit_id:?} proto-object={:?} position={:?} forward={:?} garrisoned={:?} tower-actions={tower_actions:?}",
                unit.proto_object_name,
                unit.base.position,
                unit.base.forward,
                unit.garrison.contained_unit_ids(),
            );
        }
    }
}

fn trigger_reached_selected<'a>(
    script: &'a sim::TriggerScript,
    dbids: &BTreeSet<u16>,
) -> impl Iterator<Item = &'a sim::Trigger> {
    script.triggers.iter().filter(|trigger| {
        trigger.evaluate_count > 0
            && trigger
                .effects_on_true
                .iter()
                .chain(&trigger.effects_on_false)
                .any(|effect| dbids.contains(&effect.raw_type))
    })
}

fn print_presentation_state(scenario: &str, world: &sim::World) {
    let hud = [
        sim::HudItem::Minimap,
        sim::HudItem::Resources,
        sim::HudItem::Time,
        sim::HudItem::PowerStatus,
        sim::HudItem::Units,
        sim::HudItem::DpadHelp,
        sim::HudItem::ButtonHelp,
        sim::HudItem::Reticle,
        sim::HudItem::Score,
        sim::HudItem::UnitStats,
        sim::HudItem::CircleMenuExtraInfo,
    ]
    .map(|item| (item.trigger_name(), world.hud_item_enabled(item)));
    println!(
        "{scenario}: presentation hud={hud:?} terrain-skirt={} blur={} minimap-rotation={} minimap-skirt-mirroring={} circle-menu-reset={} fade={:?} callouts={:?}",
        world.render_terrain_skirt_enabled(),
        world.screen_blur_enabled(),
        world.minimap_rotation_degrees(),
        world.minimap_skirt_mirroring(),
        world.circle_menu_reset_revision(),
        world.screen_fade_overlay(),
        world.hint_callouts().collect::<Vec<_>>(),
    );
    for player_index in 1..world.player_count() {
        let Ok(player_id) = u8::try_from(player_index) else {
            continue;
        };
        println!(
            "{scenario}: presentation player={player_id} state={:?}",
            world.player_presentation_state(player_id),
        );
    }
}

fn print_hot_triggers(scenario: &str, engine: &sim::TriggerEngine) {
    let iterator_sources = hot_iterator_sources(engine);
    let condition_variables = hot_condition_variables(engine);
    let mut hot = engine
        .scripts()
        .flat_map(|(script_id, script)| {
            script
                .triggers
                .iter()
                .filter(|trigger| trigger.evaluate_count > 1)
                .map(move |trigger| {
                    (
                        trigger.evaluate_count,
                        *script_id,
                        trigger.id,
                        trigger.editor_id,
                        trigger.name.clone(),
                        trigger.evaluate_limit,
                        trigger.evaluate_frequency,
                        trigger
                            .conditions
                            .iter()
                            .map(|condition| condition.raw_type)
                            .collect::<Vec<_>>(),
                        trigger
                            .conditions
                            .iter()
                            .map(|condition| describe_condition(condition, script))
                            .collect::<Vec<_>>(),
                        trigger
                            .effects_on_true
                            .iter()
                            .chain(&trigger.effects_on_false)
                            .map(|effect| effect.raw_type)
                            .collect::<Vec<_>>(),
                        trigger
                            .effects_on_true
                            .iter()
                            .chain(&trigger.effects_on_false)
                            .map(|effect| describe_effect(effect, script))
                            .collect::<Vec<_>>(),
                    )
                })
        })
        .collect::<Vec<_>>();
    hot.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    for (
        count,
        script_id,
        id,
        editor_id,
        name,
        evaluate_limit,
        evaluate_frequency,
        conditions,
        condition_details,
        effects,
        details,
    ) in hot.into_iter().take(25)
    {
        println!(
            "{scenario}: hot-trigger count={count} script={script_id} id={id} editor={editor_id} name={name:?} limit={evaluate_limit} frequency={evaluate_frequency} conditions={conditions:?} effects={effects:?}",
        );
        for detail in condition_details {
            println!("{scenario}:   {detail}");
        }
        for detail in details {
            println!("{scenario}:   {detail}");
        }
    }
    print_iterator_source_references(scenario, engine, &iterator_sources);
    print_condition_variable_references(scenario, engine, &condition_variables);
}

fn hot_condition_variables(engine: &sim::TriggerEngine) -> BTreeSet<(u32, u32)> {
    engine
        .scripts()
        .flat_map(|(script_id, script)| {
            script
                .triggers
                .iter()
                .filter(|trigger| trigger.evaluate_count > 1)
                .flat_map(|trigger| &trigger.conditions)
                .flat_map(|condition| condition.inputs.iter().chain(&condition.outputs))
                .map(|binding| (*script_id, binding.variable_id))
        })
        .collect()
}

fn hot_iterator_sources(engine: &sim::TriggerEngine) -> BTreeSet<(u32, u32)> {
    engine
        .scripts()
        .flat_map(|(script_id, script)| {
            script
                .triggers
                .iter()
                .filter(|trigger| trigger.evaluate_count > 1)
                .flat_map(|trigger| &trigger.conditions)
                .flat_map(|condition| condition.inputs.iter())
                .filter_map(|binding| {
                    let variable = script.get_variable(binding.variable_id)?;
                    let sim::TriggerValue::Iterator(iterator) = &variable.value else {
                        return None;
                    };
                    Some((*script_id, iterator.source_list_id()?))
                })
        })
        .collect()
}

fn print_iterator_source_references(
    scenario: &str,
    engine: &sim::TriggerEngine,
    sources: &BTreeSet<(u32, u32)>,
) {
    for (script_id, source_id) in sources {
        let Some(script) = engine.get_script(*script_id) else {
            continue;
        };
        println!(
            "{scenario}: iterator-source script={script_id} variable={source_id} value={:?}",
            script
                .get_variable(*source_id)
                .map(|variable| &variable.value)
        );
        for trigger in &script.triggers {
            for effect in trigger
                .effects_on_true
                .iter()
                .chain(&trigger.effects_on_false)
                .filter(|effect| {
                    effect
                        .inputs
                        .iter()
                        .chain(&effect.outputs)
                        .any(|binding| binding.variable_id == *source_id)
                })
            {
                println!(
                    "{scenario}:   source-ref trigger={} name={:?} {}",
                    trigger.id,
                    trigger.name,
                    describe_effect(effect, script)
                );
            }
        }
    }
}

fn print_condition_variable_references(
    scenario: &str,
    engine: &sim::TriggerEngine,
    variables: &BTreeSet<(u32, u32)>,
) {
    for (script_id, variable_id) in variables {
        let Some(script) = engine.get_script(*script_id) else {
            continue;
        };
        for trigger in &script.triggers {
            for effect in trigger
                .effects_on_true
                .iter()
                .chain(&trigger.effects_on_false)
                .filter(|effect| {
                    effect
                        .outputs
                        .iter()
                        .any(|binding| binding.variable_id == *variable_id)
                })
            {
                println!(
                    "{scenario}: condition-writer script={script_id} variable={variable_id} trigger={} name={:?} conditions={:?} effects={:?} {}",
                    trigger.id,
                    trigger.name,
                    trigger
                        .conditions
                        .iter()
                        .map(|condition| condition.raw_type)
                        .collect::<Vec<_>>(),
                    trigger
                        .effects_on_true
                        .iter()
                        .chain(&trigger.effects_on_false)
                        .map(|effect| effect.raw_type)
                        .collect::<Vec<_>>(),
                    describe_effect(effect, script)
                );
            }
        }
    }
}

fn describe_condition(condition: &sim::trigger::Condition, script: &sim::TriggerScript) -> String {
    let bindings = condition
        .inputs
        .iter()
        .chain(&condition.outputs)
        .map(|binding| describe_binding(binding.signature_id, binding.variable_id, script))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "condition id={} dbid={} version={} bindings=[{bindings}]",
        condition.id, condition.raw_type, condition.version
    )
}

fn describe_effect(effect: &sim::trigger::Effect, script: &sim::TriggerScript) -> String {
    let bindings = effect
        .inputs
        .iter()
        .chain(&effect.outputs)
        .map(|binding| describe_binding(binding.signature_id, binding.variable_id, script))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "effect id={} dbid={} version={} bindings=[{bindings}]",
        effect.id, effect.raw_type, effect.version
    )
}

fn describe_binding(signature_id: u16, variable_id: u32, script: &sim::TriggerScript) -> String {
    let variable = script.get_variable(variable_id);
    format!(
        "{signature_id}:{variable_id}=>{:?}",
        variable.map(|variable| (
            variable.name.clone(),
            variable.var_type,
            variable.is_null,
            variable.value.clone()
        ))
    )
}

fn print_catalog(scenario: &str, engine: &sim::TriggerEngine) {
    let mut conditions = BTreeSet::new();
    let mut effects = BTreeSet::new();
    let mut trigger_count = 0_usize;
    for (_, script) in engine.scripts() {
        trigger_count += script.triggers.len();
        for trigger in &script.triggers {
            conditions.extend(
                trigger
                    .conditions
                    .iter()
                    .map(|condition| condition.raw_type),
            );
            effects.extend(
                trigger
                    .effects_on_true
                    .iter()
                    .chain(&trigger.effects_on_false)
                    .map(|effect| effect.raw_type),
            );
        }
    }
    println!(
        "{scenario}: scripts={} triggers={trigger_count} conditions={conditions:?} effects={effects:?}",
        engine.script_count(),
    );
}
