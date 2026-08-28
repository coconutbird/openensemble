use pipeline::database::hw1::{Database, Tech};
use pipeline::xmb::Document;
use sim::trigger::{
    Condition, ConditionType, Effect, EffectType, Trigger, TriggerScript, TriggerValue, TriggerVar,
    VanillaLoader, VarType,
};
use sim::{EntityId, Simulation, TriggerCost, World};

#[test]
fn loader_preserves_sparse_ids_signature_slots_and_trigger_remapping() {
    let document = Document::from_xml(
        r#"<TriggerSystem Name="Sparse" Type="Scenario">
            <TriggerVars>
                <TriggerVar ID="2" Type="Integer">4</TriggerVar>
                <TriggerVar ID="7" Type="Operator">EqualTo</TriggerVar>
                <TriggerVar ID="10" Type="Integer">4</TriggerVar>
                <TriggerVar ID="21" Type="Integer">0</TriggerVar>
                <TriggerVar ID="30" Type="Trigger">42</TriggerVar>
            </TriggerVars>
            <Triggers>
                <Trigger ID="42" Active="true" EvaluateFrequency="50" EvalLimit="1">
                    <TriggerConditions><And>
                        <Condition ID="9" DBID="14">
                            <Input SigID="3">10</Input>
                            <Input SigID="1">2</Input>
                            <Input SigID="2">7</Input>
                        </Condition>
                    </And></TriggerConditions>
                    <TriggerEffectsOnTrue>
                        <Effect ID="11" DBID="52">
                            <Input SigID="1">21</Input><Output SigID="2">21</Output>
                        </Effect>
                    </TriggerEffectsOnTrue>
                    <TriggerEffectsOnFalse />
                </Trigger>
            </Triggers>
        </TriggerSystem>"#,
    )
    .expect("valid trigger XML");
    let script = VanillaLoader::from_xmb(&document).expect("valid retail trigger structure");

    assert_eq!(script.variables.len(), 5);
    assert!(script.get_variable(3).is_none());
    assert_eq!(script.triggers[0].id, 0);
    assert_eq!(script.triggers[0].editor_id, 42);
    assert_eq!(script.triggers[0].conditions[0].variable_id(1), Some(2));
    assert_eq!(script.triggers[0].conditions[0].variable_id(2), Some(7));
    assert_eq!(script.triggers[0].conditions[0].variable_id(3), Some(10));
    assert_eq!(
        script.triggers[0].effects_on_true[0].variable_id(2),
        Some(21)
    );
    assert_eq!(
        script.get_variable(30).map(|variable| &variable.value),
        Some(&TriggerValue::Trigger(0))
    );
}

#[test]
fn same_tick_activations_join_the_retail_evaluation_queue() {
    let mut world = World::new();
    let mut script = TriggerScript::default();
    script.add_variable(integer(0, 0));
    script.add_variable(trigger_variable(1, 1));
    script.add_variable(integer(2, 5));
    script.add_variable(time(3, 100));

    let activate_latecomer = Effect::new(1, EffectType::TriggerActivate).with_input(1);
    let first = Trigger::new(0)
        .starts_active()
        .with_effect_on_true(increment(0, 0))
        .with_effect_on_true(activate_latecomer);
    let latecomer = Trigger::new(1).with_effect_on_true(increment(0, 0));
    let guard = Trigger::new(2).starts_active().with_condition(
        Condition::new(2, ConditionType::GameTime)
            .with_input_at(1, 2)
            .with_input_at(2, 3),
    );
    script.add_trigger(first);
    script.add_trigger(latecomer);
    script.add_trigger(guard);
    let script_id = world.trigger_engine_mut().add_script(script);
    world.trigger_engine_mut().activate_script(script_id, 0);

    let update = world.update_triggers();

    assert_eq!(update.evaluations, 3);
    assert_eq!(update.triggers_fired, 2);
    assert_eq!(update.effects_applied, 3);
    assert_eq!(script_integer(&world, script_id, 0), 2);
    assert_eq!(world.trigger_engine().active_trigger_count(), 1);
}

#[test]
fn conditional_false_branches_execute_against_sparse_variables() {
    let mut world = World::new();
    let mut script = TriggerScript::default();
    script.add_variable(integer(0, 1));
    script.add_variable(integer(4, 3));
    script.add_variable(integer(9, 2));
    script.add_variable(integer(20, 10));
    script.add_variable(integer(30, 5));
    script.add_variable(time(31, 100));
    let mut conditional = Trigger::new(0)
        .starts_active()
        .with_condition(
            Condition::new(0, ConditionType::CompareCount)
                .with_input_at(1, 0)
                .with_input_at(2, 4)
                .with_input_at(3, 9),
        )
        .with_effect_on_false(increment(20, 20));
    conditional.is_conditional = true;
    let guard = Trigger::new(1).starts_active().with_condition(
        Condition::new(1, ConditionType::GameTime)
            .with_input_at(1, 30)
            .with_input_at(2, 31),
    );
    script.add_trigger(conditional);
    script.add_trigger(guard);
    let script_id = world.trigger_engine_mut().add_script(script);
    world.trigger_engine_mut().activate_script(script_id, 0);

    let update = world.update_triggers();

    assert_eq!(update.evaluations, 2);
    assert_eq!(update.triggers_fired, 1);
    assert_eq!(script_integer(&world, script_id, 20), 11);
    assert_eq!(world.trigger_engine().active_trigger_count(), 1);
}

#[test]
fn deactivation_before_effects_allows_bounded_self_reactivation() {
    let mut world = World::new();
    let mut script = TriggerScript::default();
    script.add_variable(integer(0, 0));
    script.add_variable(trigger_variable(1, 0));
    let reactivate = Effect::new(1, EffectType::TriggerActivate).with_input(1);
    let mut trigger = Trigger::new(0)
        .starts_active()
        .with_effect_on_true(increment(0, 0))
        .with_effect_on_true(reactivate);
    trigger.evaluate_limit = 3;
    script.add_trigger(trigger);
    let script_id = world.trigger_engine_mut().add_script(script);
    world.trigger_engine_mut().activate_script(script_id, 0);

    let first = world.update_triggers();
    assert_eq!(first.evaluations, 3);
    assert_eq!(script_integer(&world, script_id, 0), 3);
    assert_eq!(world.trigger_engine().active_trigger_count(), 1);
    let second = world.update_triggers();
    assert_eq!(second.evaluations, 3);
    assert_eq!(script_integer(&world, script_id, 0), 6);
}

#[test]
fn unsupported_conditions_are_reported_and_never_fire() {
    let mut world = World::new();
    let mut script = TriggerScript::default();
    let mut condition = Condition::new(0, ConditionType::Custom);
    condition.raw_type = 4_000;
    script.add_trigger(Trigger::new(0).starts_active().with_condition(condition));
    let script_id = world.trigger_engine_mut().add_script(script);
    world.trigger_engine_mut().activate_script(script_id, 0);

    let update = world.update_triggers();

    assert_eq!(update.evaluations, 1);
    assert_eq!(update.triggers_fired, 0);
    assert_eq!(update.unsupported_condition_types, vec![4_000]);
    assert_eq!(world.trigger_engine().active_trigger_count(), 1);
}

#[test]
fn simulation_tick_executes_authoritative_teleporter_setup() {
    let mut world = World::new();
    let source = world.create_squad(0);
    let target = world.create_squad(0);
    let mut script = TriggerScript::default();
    script.add_variable(entity(0, source));
    script.add_variable(entity(1, target));
    script.add_trigger(
        Trigger::new(0).starts_active().with_effect_on_true(
            Effect::new(0, EffectType::SetTeleporterDestination)
                .with_input_at(1, 0)
                .with_input_at(2, 1),
        ),
    );
    let script_id = world.trigger_engine_mut().add_script(script);
    world.trigger_engine_mut().activate_script(script_id, 0);

    Simulation::new().tick_with_world(&mut world);

    assert_eq!(world.game_time(), 50);
    assert_eq!(
        world.get_squad(source).unwrap().teleporter_destination,
        Some(target)
    );
    assert_eq!(world.trigger_engine().script_count(), 0);
}

#[test]
fn database_tick_supplies_technology_context_to_triggers() {
    let mut database = Database::new();
    database.techs.push(Tech {
        name: "test_upgrade".to_owned(),
        ..Tech::default()
    });
    let mut world = World::new();
    world.init_players(1);
    let mut script = TriggerScript::default();
    script.add_variable(integer(0, 0));
    script.add_variable(TriggerVar::new(1, VarType::Player).with_value(TriggerValue::Player(1)));
    script.add_variable(TriggerVar::new(2, VarType::Tech).with_value(TriggerValue::Tech(0)));
    script.add_variable(TriggerVar::new(3, VarType::TechStatus).with_value(TriggerValue::Int(2)));
    script.add_variable(integer(4, 5));
    script.add_variable(time(5, 100));
    let mut condition = Condition::new(0, ConditionType::TechStatus)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3);
    condition.version = 1;
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_condition(condition)
            .with_effect_on_true(increment(0, 0)),
    );
    script.add_trigger(
        Trigger::new(1).starts_active().with_condition(
            Condition::new(1, ConditionType::GameTime)
                .with_input_at(1, 4)
                .with_input_at(2, 5),
        ),
    );
    let script_id = world.trigger_engine_mut().add_script(script);
    world.trigger_engine_mut().activate_script(script_id, 0);

    Simulation::new().tick_with_world_and_database(&mut world, &database);

    assert_eq!(script_integer(&world, script_id, 0), 1);
}

#[test]
fn resource_and_technology_effects_mutate_authoritative_player_state() {
    let mut database = Database::new();
    database.techs.push(Tech {
        name: "test_upgrade".to_owned(),
        ..Tech::default()
    });
    let mut world = World::new();
    world.init_players(1);
    let mut script = TriggerScript::default();
    script.add_variable(player(0, 1));
    script.add_variable(cost(1, 10.0, 20.0, 30.0));
    script.add_variable(cost(2, 3.0, 4.0, 5.0));
    script.add_variable(cost(3, 1.0, 2.0, 3.0));
    script.add_variable(cost(4, 0.0, 0.0, 0.0).as_output());
    script.add_variable(cost(5, 100.0, 200.0, 300.0));
    script.add_variable(cost(6, 0.0, 0.0, 0.0).as_output());
    script.add_variable(TriggerVar::new(7, VarType::Tech).with_value(TriggerValue::Tech(0)));
    script.add_variable(integer(20, 5));
    script.add_variable(time(21, 100));
    let trigger = Trigger::new(0)
        .starts_active()
        .with_effect_on_true(
            Effect::new(0, EffectType::SetResources)
                .with_input_at(1, 0)
                .with_input_at(2, 1),
        )
        .with_effect_on_true(
            Effect::new(1, EffectType::PayCost)
                .with_input_at(1, 0)
                .with_input_at(2, 2),
        )
        .with_effect_on_true(
            Effect::new(2, EffectType::RefundCost)
                .with_input_at(1, 0)
                .with_input_at(2, 3),
        )
        .with_effect_on_true(
            Effect::new(3, EffectType::GetResources)
                .with_input_at(1, 0)
                .with_output_at(2, 4),
        )
        .with_effect_on_true(
            Effect::new(4, EffectType::SetResourcesTotals)
                .with_input_at(1, 0)
                .with_input_at(2, 5),
        )
        .with_effect_on_true(
            Effect::new(5, EffectType::GetResourcesTotals)
                .with_input_at(1, 0)
                .with_output_at(2, 6),
        )
        .with_effect_on_true(
            Effect::new(6, EffectType::TechActivate)
                .with_input_at(1, 0)
                .with_input_at(2, 7),
        );
    script.add_trigger(trigger);
    add_guard(&mut script, 20, 21);
    let script_id = world.trigger_engine_mut().add_script(script);
    world.trigger_engine_mut().activate_script(script_id, 0);

    let update = world.update_triggers_with_database(&database);

    assert_eq!(update.effects_applied, 7);
    assert_cost_eq(script_cost(&world, script_id, 4), [8.0, 18.0, 28.0]);
    assert_cost_eq(script_cost(&world, script_id, 6), [100.0, 200.0, 300.0]);
    let player = world.get_player(1).unwrap();
    assert_eq!(player.resources.amounts[..3], [8.0, 18.0, 28.0]);
    assert_eq!(player.total_resources.amounts[..3], [100.0, 200.0, 300.0]);
    assert!(player.technologies.is_active("test_upgrade"));
}

#[test]
fn unaffordable_pay_cost_is_a_supported_no_op_and_tech_can_deactivate() {
    let mut database = Database::new();
    database.techs.push(Tech {
        name: "test_upgrade".to_owned(),
        ..Tech::default()
    });
    let mut world = World::new();
    world.init_players(1);
    world.get_player_mut(1).unwrap().resources.amounts[..3].copy_from_slice(&[2.0, 3.0, 4.0]);
    world
        .activate_technology(1, &database, "test_upgrade")
        .unwrap();
    let mut script = TriggerScript::default();
    script.add_variable(player(0, 1));
    script.add_variable(cost(1, 20.0, 0.0, 0.0));
    script.add_variable(TriggerVar::new(2, VarType::Tech).with_value(TriggerValue::Tech(0)));
    script.add_variable(integer(20, 5));
    script.add_variable(time(21, 100));
    script.add_trigger(
        Trigger::new(0)
            .starts_active()
            .with_effect_on_true(
                Effect::new(0, EffectType::PayCost)
                    .with_input_at(1, 0)
                    .with_input_at(2, 1),
            )
            .with_effect_on_true(
                Effect::new(1, EffectType::TechDeactivate)
                    .with_input_at(1, 0)
                    .with_input_at(2, 2),
            ),
    );
    add_guard(&mut script, 20, 21);
    let script_id = world.trigger_engine_mut().add_script(script);
    world.trigger_engine_mut().activate_script(script_id, 0);

    let update = world.update_triggers_with_database(&database);

    assert_eq!(update.effects_applied, 2);
    let player = world.get_player(1).unwrap();
    assert_eq!(player.resources.amounts[..3], [2.0, 3.0, 4.0]);
    assert!(!player.technologies.is_active("test_upgrade"));
}

#[test]
fn trigger_variables_and_activation_are_part_of_the_world_checksum() {
    let mut world = World::new();
    let mut script = TriggerScript::default();
    script.add_variable(integer(4, 7));
    script.add_trigger(Trigger::new(0));
    let script_id = world.trigger_engine_mut().add_script(script);
    let initial = world.checksum();
    world
        .trigger_engine_mut()
        .get_script_mut(script_id)
        .unwrap()
        .get_variable_mut(4)
        .unwrap()
        .value = TriggerValue::Int(8);
    let variable_changed = world.checksum();
    world.trigger_engine_mut().activate_script(script_id, 123);

    assert_ne!(initial, variable_changed);
    assert_ne!(variable_changed, world.checksum());
}

fn integer(id: u32, value: i32) -> TriggerVar {
    TriggerVar::new(id, VarType::Integer).with_value(TriggerValue::Int(value))
}

fn time(id: u32, value: u32) -> TriggerVar {
    TriggerVar::new(id, VarType::Time).with_value(TriggerValue::Time(value))
}

fn trigger_variable(id: u32, trigger_id: u32) -> TriggerVar {
    TriggerVar::new(id, VarType::Trigger).with_value(TriggerValue::Trigger(trigger_id))
}

fn entity(id: u32, entity_id: EntityId) -> TriggerVar {
    TriggerVar::new(id, VarType::Squad).with_value(TriggerValue::Squad(entity_id))
}

fn player(id: u32, player_id: i32) -> TriggerVar {
    TriggerVar::new(id, VarType::Player).with_value(TriggerValue::Player(player_id))
}

fn cost(id: u32, supplies: f32, power: f32, population: f32) -> TriggerVar {
    TriggerVar::new(id, VarType::Cost).with_value(TriggerValue::Cost(TriggerCost {
        supplies,
        power,
        population,
        resource_3: 0.0,
    }))
}

fn add_guard(script: &mut TriggerScript, operator_id: u32, time_id: u32) {
    script.add_trigger(
        Trigger::new(1).starts_active().with_condition(
            Condition::new(1, ConditionType::GameTime)
                .with_input_at(1, operator_id)
                .with_input_at(2, time_id),
        ),
    );
}

fn increment(source: u32, destination: u32) -> Effect {
    Effect::new(0, EffectType::CountIncrement)
        .with_input_at(1, source)
        .with_output_at(2, destination)
}

fn script_integer(world: &World, script_id: u32, variable_id: u32) -> i32 {
    world
        .trigger_engine()
        .get_script(script_id)
        .and_then(|script| script.get_variable(variable_id))
        .and_then(|variable| variable.value.as_int())
        .expect("integer trigger variable should exist")
}

fn script_cost(world: &World, script_id: u32, variable_id: u32) -> [f32; 3] {
    match &world
        .trigger_engine()
        .get_script(script_id)
        .and_then(|script| script.get_variable(variable_id))
        .expect("cost trigger variable should exist")
        .value
    {
        TriggerValue::Cost(cost) => [cost.supplies, cost.power, cost.population],
        value => panic!("expected Cost, got {value:?}"),
    }
}

fn assert_cost_eq(actual: [f32; 3], expected: [f32; 3]) {
    assert_eq!(actual.map(f32::to_bits), expected.map(f32::to_bits));
}
