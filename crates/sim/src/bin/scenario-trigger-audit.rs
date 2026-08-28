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
                print_catalog(&scenario, loaded.simulation.world.trigger_engine());
                print_selected_bindings(
                    &scenario,
                    loaded.simulation.world.trigger_engine(),
                    &selected_dbids,
                );
                let update = loaded.simulation.world.update_triggers_with_gameplay(
                    &loaded.content.database,
                    &loaded.simulation.gameplay,
                );
                println!("{scenario}: initial-update={update:?}");
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
        variable.map(|variable| (variable.var_type, variable.is_null, variable.value.clone()))
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
