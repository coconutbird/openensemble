use era::{TeaKeys, Writer};
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const BASE_GAME_DATA: &str = r"<GameData>
    <DifficultyDefault>0.25</DifficultyDefault>
    <ConstructionDamageMultiplier>0.5</ConstructionDamageMultiplier>
    <DefaultCryoPoints>10</DefaultCryoPoints>
    <FreezingSpeedModifier>0.1</FreezingSpeedModifier>
</GameData>";

const BASE_CONFIG: &str = "Veterancy\nBaseOnlyConfig\n";
const SCENARIO_CONFIG: &str = "-Veterancy\nScenarioOnlyConfig\n";

const SCENARIO_GAME_DATA: &str = r"<GameData>
    <DifficultyDefault>0.82</DifficultyDefault>
    <ConstructionDamageMultiplier>3.0</ConstructionDamageMultiplier>
    <TimeFrozenToThaw>8</TimeFrozenToThaw>
    <TimeFreezingToThaw>3</TimeFreezingToThaw>
    <DefaultCryoPoints>66</DefaultCryoPoints>
    <DefaultThawSpeed>11</DefaultThawSpeed>
    <FreezingSpeedModifier>0.4</FreezingSpeedModifier>
    <FreezingDamageModifier>1.5</FreezingDamageModifier>
    <FrozenDamageModifier>2</FrozenDamageModifier>
</GameData>";

const BASE_SQUADS: &str = r#"<Squads>
    <Squad name="base_only" />
</Squads>"#;

const SCENARIO_SQUADS: &str = r#"<Squads>
    <Squad name="scenario_joiner"><CryoPoints>77</CryoPoints></Squad>
    <Squad name="scenario_target" />
    <Squad name="scenario_death_spawn" dbid="203">
        <Units>
            <Unit count="2">scenario_death_member</Unit>
        </Units>
    </Squad>
    <MergedSquads>scenario_joiner
        <MergedSquad>scenario_target</MergedSquad>
    </MergedSquads>
</Squads>"#;

const SCENARIO_OBJECTS: &str = r#"<Objects>
    <Object name="variation_crate">
        <ObjectClass>Object</ObjectClass>
        <Visual>variation_crate</Visual>
    </Object>
    <Object name="scenario_minelayer">
        <ObjectClass>Unit</ObjectClass>
        <ObjectType>CanCryo</ObjectType>
        <Tactics>scenario_minelayer.tactics</Tactics>
        <AbilityCommand>ScenarioMines</AbilityCommand>
        <Hitpoints>40</Hitpoints>
        <AmmoMax>20</AmmoMax>
        <Flag>StartAtMaxAmmo</Flag>
    </Object>
    <Object name="scenario_mine">
        <ObjectClass>Unit</ObjectClass>
        <Hitpoints>5</Hitpoints>
    </Object>
    <Object name="scenario_vehicle">
        <ObjectClass>Unit</ObjectClass>
        <PhysicsInfo>layered_vehicle</PhysicsInfo>
        <Hitpoints>200</Hitpoints>
        <Velocity>20</Velocity>
        <TurnRate>90</TurnRate>
        <ObstructionRadiusX>8</ObstructionRadiusX>
        <ObstructionRadiusY>4</ObstructionRadiusY>
        <ObstructionRadiusZ>8</ObstructionRadiusZ>
    </Object>
    <Object name="scenario_death_source" dbid="104">
        <ObjectClass>Unit</ObjectClass>
        <Hitpoints>50</Hitpoints>
        <DeathSpawnSquad>scenario_death_spawn</DeathSpawnSquad>
    </Object>
    <Object name="scenario_death_member" dbid="105">
        <ObjectClass>Unit</ObjectClass>
        <Hitpoints>25</Hitpoints>
    </Object>
    <Object name="scenario_replacement_source" dbid="106">
        <ObjectClass>Unit</ObjectClass>
        <Hitpoints>60</Hitpoints>
        <DeathReplacement>scenario_replacement_target</DeathReplacement>
    </Object>
    <Object name="scenario_replacement_target" dbid="107">
        <ObjectClass>Building</ObjectClass>
        <Hitpoints>333</Hitpoints>
        <ObstructionRadiusX>4</ObstructionRadiusX>
        <ObstructionRadiusY>5</ObstructionRadiusY>
        <ObstructionRadiusZ>6</ObstructionRadiusZ>
    </Object>
    <Object name="scenario_cryo_fx">
        <ObjectClass>Object</ObjectClass>
        <Visual>scenario_cryo_fx</Visual>
        <Lifespan>0.2</Lifespan>
    </Object>
    <Object name="scenario_cryo_bomber">
        <ObjectClass>Object</ObjectClass>
        <Visual>scenario_cryo_bomber</Visual>
    </Object>
</Objects>"#;

const SCENARIO_POWERS: &str = r#"<Powers>
    <Power name="ScenarioCryo">
        <Attributes>
            <PowerType>Cryo</PowerType>
            <BaseDataLevel>
                <Data type="protoobject" name="CryoObject">scenario_cryo_fx</Data>
                <Data type="float" name="CryoRadius">45</Data>
                <Data type="float" name="MinCryoFalloff">0.25</Data>
                <Data type="objecttype" name="FilterType">CanCryo</Data>
                <Data type="float" name="TickDuration">0.05</Data>
                <Data type="int" name="NumTicks">2</Data>
                <Data type="float" name="CryoAmountPerTick">1</Data>
                <Data type="float" name="EffectStartTime">0</Data>
                <Data type="float" name="MaxKillHp">1</Data>
                <Data type="float" name="FreezingThawTime">5</Data>
                <Data type="float" name="FrozenThawTime">7</Data>
                <Data type="protoobject" name="Bomber">scenario_cryo_bomber</Data>
                <Data type="float" name="BomberBombTime">0.1</Data>
                <Data type="float" name="BomberFlyinDistance">100</Data>
                <Data type="float" name="BomberFlyinHeight">30</Data>
                <Data type="float" name="BomberBombHeight">10</Data>
                <Data type="float" name="BomberSpeed">50</Data>
                <Data type="float" name="BomberFlyOutTime">0.5</Data>
            </BaseDataLevel>
            <DataLevel level="0">
                <Data type="float" name="FrozenThawTime">13</Data>
            </DataLevel>
        </Attributes>
    </Power>
</Powers>"#;

const SCENARIO_ABILITIES: &str = r#"<Abilities>
    <Ability Name="Command" />
    <Ability Name="ScenarioMines">
        <Type>Work</Type>
        <TargetType>Location</TargetType>
        <Object>scenario_mine</Object>
        <AmmoCost>7.5</AmmoCost>
    </Ability>
</Abilities>"#;

const SCENARIO_CIVS: &str = r"<Civs>
    <Civ><Name>ScenarioCiv</Name><CivTech>ScenarioCivBootstrap</CivTech></Civ>
</Civs>";

const SCENARIO_LEADERS: &str = r#"<Leaders>
    <Leader Name="ScenarioLeader">
        <Civ>ScenarioCiv</Civ>
        <Tech>ScenarioLeaderBootstrap</Tech>
    </Leader>
</Leaders>"#;

const SCENARIO_TECHS: &str = r#"<TechTree>
    <Tech name="ScenarioCivBootstrap">
        <Effects>
            <Effect type="Data" subtype="Hitpoints" amount="2" relativity="Percent">
                <Target type="ProtoUnit">scenario_minelayer</Target>
            </Effect>
        </Effects>
    </Tech>
    <Tech name="ScenarioCivShadow">
        <Flag>Shadow</Flag>
        <Prereqs><TechStatus tech="ScenarioCivBootstrap" status="Active" /></Prereqs>
    </Tech>
    <Tech name="ScenarioLeaderBootstrap" />
</TechTree>"#;

const SCENARIO_MINELAYER_TACTICS: &str = r"<TacticData>
    <Action>
        <Name>ScenarioPlaceMine</Name>
        <ActionType>Mines</ActionType>
        <WorkRange>12</WorkRange>
    </Action>
    <Tactic>
        <TargetRule>
            <Relation>Any</Relation>
            <Action>ScenarioPlaceMine</Action>
            <Ability>Command</Ability>
        </TargetRule>
    </Tactic>
</TacticData>";

const BASE_VEHICLE_PHYSICS: &str = r"<physics>
    <blueprint>layered_vehicle</blueprint>
    <Vehicle>warthog</Vehicle>
    <CenterOffset>9,9,9</CenterOffset>
</physics>";

const SCENARIO_VEHICLE_PHYSICS: &str = r"<physics>
    <blueprint>layered_vehicle</blueprint>
    <Vehicle>ghost</Vehicle>
    <CenterOffset>1,2,3</CenterOffset>
</physics>";

const BASE_VEHICLE_BLUEPRINT: &str = r"<blueprint>
    <mass>10</mass>
    <friction>0.1</friction>
    <restitution>0.05</restitution>
    <linearDamping>0.2</linearDamping>
    <angularDamping>0.3</angularDamping>
    <shape>layered_vehicle</shape>
</blueprint>";

const SCENARIO_VEHICLE_BLUEPRINT: &str = r"<blueprint>
    <mass>222</mass>
    <friction>1.25</friction>
    <restitution>0.4</restitution>
    <linearDamping>0.05</linearDamping>
    <angularDamping>0.15</angularDamping>
    <shape>layered_vehicle</shape>
</blueprint>";

const BASE_VEHICLE_SHAPE: &str = r#"<hke version="V_20200_B_20031014">
    <hkobject name="body" type="hkBoxShape">
        <hkparam name="halfExtents" type="hkTypeVector4">(9 9 9)</hkparam>
    </hkobject>
</hke>"#;

const SCENARIO_VEHICLE_SHAPE: &str = r#"<hke version="V_20200_B_20031014">
    <hkobject name="body" type="hkBoxShape">
        <hkparam name="halfExtents" type="hkTypeVector4">(4 5 6)</hkparam>
    </hkobject>
</hke>"#;

const SCENARIO_DESCRIPTIONS: &str = r#"<ScenarioDescriptions>
    <ScenarioInfo
        File="skirmish\design\layered_test\layered_test.scn"
        Type="Skirmish"
        MaxPlayers="2"
    />
</ScenarioDescriptions>"#;

const SCENARIO: &str = r#"<Scenario>
    <AllowVeterancy>false</AllowVeterancy>
    <Players>
        <Player Name="Scenario Player" Civ="ScenarioCiv" Leader1="ScenarioLeader"
            Team="1" Controllable="true" />
    </Players>
    <Objects>
        <Object Player="1" ID="99" VisualVariationIndex="1">variation_crate</Object>
        <Object Player="1" ID="100">variation_crate</Object>
    </Objects>
</Scenario>"#;

const VETERANCY_SCENARIO: &str = r#"<Scenario>
    <AllowVeterancy>true</AllowVeterancy>
    <Players><Player Name="Scenario Player" Team="1" /></Players>
</Scenario>"#;

#[test]
fn scenario_database_tables_win_before_authoritative_simulation_is_built() {
    let game_dir = TemporaryGameDir::create();
    write_archive(
        &game_dir.path().join("root.era"),
        &[
            ("data\\gamedata.xml.xmb", BASE_GAME_DATA),
            ("data\\squads.xml.xmb", BASE_SQUADS),
            ("data\\scenariodescriptions.xml.xmb", SCENARIO_DESCRIPTIONS),
            ("startup\\game.cfg", BASE_CONFIG),
            ("physics\\layered_vehicle.physics.xmb", BASE_VEHICLE_PHYSICS),
            (
                "physics\\layered_vehicle.blueprint.xmb",
                BASE_VEHICLE_BLUEPRINT,
            ),
            ("physics\\layered_vehicle.shp.xmb", BASE_VEHICLE_SHAPE),
        ],
    );
    write_archive(
        &game_dir.path().join("layered_test.era"),
        &[
            ("data\\gamedata.xml.xmb", SCENARIO_GAME_DATA),
            ("data\\abilities.xml.xmb", SCENARIO_ABILITIES),
            ("data\\civs.xml.xmb", SCENARIO_CIVS),
            ("data\\leaders.xml.xmb", SCENARIO_LEADERS),
            ("data\\objects.xml.xmb", SCENARIO_OBJECTS),
            ("data\\powers.xml.xmb", SCENARIO_POWERS),
            ("data\\squads.xml.xmb", SCENARIO_SQUADS),
            ("data\\techs.xml.xmb", SCENARIO_TECHS),
            (
                "data\\tactics\\scenario_minelayer.tactics.xmb",
                SCENARIO_MINELAYER_TACTICS,
            ),
            (
                "physics\\layered_vehicle.physics.xmb",
                SCENARIO_VEHICLE_PHYSICS,
            ),
            (
                "physics\\layered_vehicle.blueprint.xmb",
                SCENARIO_VEHICLE_BLUEPRINT,
            ),
            ("physics\\layered_vehicle.shp.xmb", SCENARIO_VEHICLE_SHAPE),
            ("startup\\game.cfg", SCENARIO_CONFIG),
            (
                "scenario\\skirmish\\design\\layered_test\\layered_test.scn.xmb",
                SCENARIO,
            ),
        ],
    );

    let game_dir_text = game_dir.path().to_string_lossy();
    let mut loaded = sim::load_scenario_from_game_dir(&game_dir_text, "layered_test")
        .expect("synthetic scenario and database should load together");

    assert_layered_database_and_scenario_state(&loaded);
    assert_layered_vehicle_physics(&mut loaded);
    assert_layered_cryo_execution(&mut loaded);
    assert_layered_mines_execution(&mut loaded);
    assert_layered_death_spawn_execution(&mut loaded);
    assert_layered_death_replacement_execution(&mut loaded);
    assert_layered_database_provenance(&loaded);
}

fn assert_layered_database_and_scenario_state(loaded: &sim::LoadedGameScenario) {
    assert_startup_config_precedes_scenario_archive(loaded);
    let game_data = loaded
        .content
        .database
        .game_data
        .as_ref()
        .expect("scenario game data");
    assert_eq!(game_data.difficulty_default, Some(0.82));
    assert_eq!(game_data.construction_damage_multiplier, Some(3.0));
    assert_eq!(game_data.default_cryo_points, Some(66.0));
    assert_eq!(game_data.freezing_speed_modifier, Some(0.4));
    assert_eq!(loaded.content.database.squads.len(), 3);
    assert_eq!(loaded.content.database.objects.len(), 10);
    assert_eq!(loaded.content.database.powers.len(), 1);
    assert!(
        loaded
            .content
            .database
            .squads
            .iter()
            .all(|squad| squad.name != "base_only")
    );

    let scenario_player = loaded
        .simulation
        .world
        .get_player(1)
        .expect("scenario player");
    assert_eq!(scenario_player.name, "Scenario Player");
    assert!((scenario_player.difficulty - 0.82).abs() < f32::EPSILON);
    assert_eq!(scenario_player.civ_id, 0);
    assert_eq!(scenario_player.leader_id, 0);
    assert_eq!(
        scenario_player
            .technologies
            .active_technologies()
            .collect::<Vec<_>>(),
        [
            "ScenarioCivBootstrap",
            "ScenarioCivShadow",
            "ScenarioLeaderBootstrap"
        ]
    );

    let explicit_variation = loaded.simulation.get_entity_id(99).unwrap();
    let random_variation = loaded.simulation.get_entity_id(100).unwrap();
    assert_eq!(
        loaded
            .simulation
            .world
            .get_object(explicit_variation)
            .unwrap()
            .object_state
            .visual_variation_index(),
        Some(1)
    );
    assert_eq!(
        loaded
            .simulation
            .world
            .get_object(random_variation)
            .unwrap()
            .object_state
            .visual_variation_index(),
        None
    );

    let merged = loaded
        .simulation
        .gameplay
        .merged_squad_profile("scenario_joiner", "scenario_target")
        .expect("scenario-local raw MergedSquads mapping");
    assert_eq!(
        merged.proto_squad_name(),
        "merged_scenario_target_scenario_joiner"
    );
    assert_eq!(merged.proto_squad_id(), 3);
}

fn assert_layered_cryo_execution(loaded: &mut sim::LoadedGameScenario) {
    let squad_id = loaded.simulation.world.create_squad_at(1, glam::Vec3::ZERO);
    "scenario_joiner".clone_into(
        &mut loaded
            .simulation
            .world
            .get_squad_mut(squad_id)
            .unwrap()
            .proto_squad_name,
    );
    let unit_id = loaded.simulation.world.create_unit(1);
    assert!(
        loaded
            .simulation
            .world
            .attach_unit_to_squad(unit_id, squad_id)
    );

    assert!(
        loaded
            .simulation
            .world
            .add_squad_cryo(squad_id, 7.0, &loaded.content.database)
    );
    let squad = loaded.simulation.world.get_squad_mut(squad_id).unwrap();
    assert_eq!(squad.cryo_state(), sim::SquadCryoState::Freezing);
    assert_eq!(squad.maximum_cryo_points().to_bits(), 77.0_f32.to_bits());
    squad.move_to(glam::Vec3::new(100.0, 0.0, 0.0));
    assert!(!squad.update_movement(1.0));
    assert_close(squad.base.position.x, 4.0);

    assert!(
        loaded
            .simulation
            .world
            .add_squad_cryo(squad_id, 70.0, &loaded.content.database)
    );
    assert!(
        loaded
            .simulation
            .world
            .get_squad(squad_id)
            .unwrap()
            .is_cryo_frozen()
    );
    assert!(
        loaded
            .simulation
            .world
            .get_unit(unit_id)
            .unwrap()
            .is_shatter_on_death()
    );

    assert_layered_cryo_power_execution(loaded);
}

fn assert_layered_cryo_power_execution(loaded: &mut sim::LoadedGameScenario) {
    let power_target = loaded
        .simulation
        .world
        .create_squad_at(1, glam::Vec3::new(5.0, 0.0, 0.0));
    "scenario_joiner".clone_into(
        &mut loaded
            .simulation
            .world
            .get_squad_mut(power_target)
            .unwrap()
            .proto_squad_name,
    );
    let target_unit = sim::spawn_object_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        1,
        sim::object_prototype_id(&loaded.content.database, "scenario_minelayer").unwrap(),
        glam::Vec3::new(5.0, 0.0, 0.0),
        glam::Vec3::X,
    )
    .expect("scenario-local CanCryo target");
    assert!(
        loaded
            .simulation
            .world
            .attach_unit_to_squad(target_unit, power_target)
    );

    let (_, first_tick) = start_layered_cryo_power(loaded);
    loaded.simulation.world.game_time_ms = first_tick;
    loaded
        .simulation
        .world
        .update_entities_with_database_and_gameplay(
            0.05,
            &loaded.content.database,
            &loaded.simulation.gameplay,
        );
    assert!(
        !loaded
            .simulation
            .world
            .get_squad(power_target)
            .unwrap()
            .is_cryo_frozen()
    );
    loaded.simulation.world.game_time_ms = first_tick.wrapping_add(50);
    loaded
        .simulation
        .world
        .update_entities_with_database_and_gameplay(
            0.05,
            &loaded.content.database,
            &loaded.simulation.gameplay,
        );
    let target = loaded.simulation.world.get_squad(power_target).unwrap();
    assert!(target.is_cryo_frozen());
    assert_eq!(target.maximum_cryo_points().to_bits(), 77.0_f32.to_bits());
    assert!(target.cryo_thaw_delay() > 12.9);
    let cryo_object_id = loaded.simulation.world.active_cryo_powers()[0].cryo_object_id();
    assert_eq!(
        loaded
            .simulation
            .world
            .get_object(cryo_object_id)
            .unwrap()
            .proto_object_name,
        "scenario_cryo_fx"
    );
}

fn start_layered_cryo_power(loaded: &mut sim::LoadedGameScenario) -> (sim::EntityId, u32) {
    let mut command = sim::PowerCommand::default();
    command.base.player_id = 1;
    command
        .base
        .set_flag(sim::commands::power_command_flags::NO_COST, true);
    command.power_type = sim::PowerCommandType::InvokePower2;
    command.proto_power_id = sim::power_prototype_id(&loaded.content.database, "ScenarioCryo")
        .expect("scenario-local power ID");
    command.power_level = 0;
    command.target_location = glam::Vec4::ZERO;
    command.squad_id = sim::EntityId::INVALID;
    sim::CommandExecutor::with_database(&loaded.content.database).execute(
        &mut loaded.simulation.world,
        &sim::CommandEntry {
            command: sim::QueuedCommand::Power(command),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        },
    );
    let execution = loaded
        .simulation
        .world
        .active_cryo_powers()
        .first()
        .expect("InvokePower2 should start scenario-layered native Cryo");
    assert_eq!(execution.cryo_object_prototype(), "scenario_cryo_fx");
    assert_eq!(execution.bomber_prototype(), "scenario_cryo_bomber");
    assert_eq!(
        execution.cryo_amount_per_tick().to_bits(),
        1.0_f32.to_bits()
    );
    let bomber_id = execution.bomber_object_id();
    assert_eq!(
        loaded
            .simulation
            .world
            .get_object(bomber_id)
            .unwrap()
            .proto_object_name,
        "scenario_cryo_bomber"
    );
    (bomber_id, execution.next_tick_time_ms())
}

fn assert_layered_vehicle_physics(loaded: &mut sim::LoadedGameScenario) {
    assert!(
        loaded
            .simulation
            .gameplay
            .vehicle_physics_issues()
            .is_empty()
    );
    let profile = loaded
        .simulation
        .gameplay
        .ground_vehicle_physics("SCENARIO_VEHICLE")
        .expect("scenario vehicle physics profile");
    assert_eq!(profile.physics_info(), "layered_vehicle");
    assert_eq!(profile.kind(), sim::GroundVehicleKind::Ghost);
    assert_eq!(
        profile.collider().half_extents,
        glam::Vec3::new(4.0, 5.0, 6.0)
    );
    assert_eq!(
        profile.collider().center_offset,
        glam::Vec3::new(1.0, 2.0, 3.0)
    );
    assert_close(profile.material().mass, 222.0);
    assert_close(profile.material().friction, 1.25);
    assert_close(profile.material().restitution, 0.4);
    assert_close(profile.material().linear_damping, 0.05);
    assert_close(profile.material().angular_damping, 0.15);

    let vehicle_id = sim::spawn_object_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        1,
        sim::object_prototype_id(&loaded.content.database, "scenario_vehicle").unwrap(),
        glam::Vec3::new(10.0, 7.0, 20.0),
        glam::Vec3::Z,
    )
    .expect("scenario-layered vehicle prototype");
    let vehicle = loaded.simulation.world.get_unit(vehicle_id).unwrap();
    assert_close(vehicle.speed, 20.0);
    assert_close(vehicle.acceleration, 50.0);
    assert_close(vehicle.turn_rate_degrees, 90.0);
    let body = vehicle.physics.as_ref().expect("dynamic Ghost body");
    assert_eq!(body.motion_type(), sim::MotionType::Dynamic);
    assert_eq!(body.collider(), profile.collider());
    assert_eq!(body.material(), profile.material());
    assert_close(body.max_speed(), 20.0);
    assert_close(body.acceleration(), 50.0);
    assert_close(body.turn_rate_degrees(), 90.0);
}

fn assert_layered_mines_execution(loaded: &mut sim::LoadedGameScenario) {
    let ability = loaded
        .simulation
        .gameplay
        .resolve_order_ability("scenario_minelayer", 0)
        .expect("scenario-layered AbilityCommand");
    assert_eq!(ability.name(), "ScenarioMines");
    assert_eq!(ability.ability_type(), Some("Work"));
    assert_eq!(ability.target_type(), Some("Location"));
    assert_eq!(ability.objects(), ["scenario_mine"]);
    assert!((ability.ammunition_cost() - 7.5).abs() < f32::EPSILON);
    let mine_profile = loaded
        .simulation
        .gameplay
        .select_mine_action(
            "scenario_minelayer",
            &sim::AttackQuery {
                relation: sim::TacticRelation::SelfPlayer,
                squad_mode: sim::SquadMode::Normal,
                ability_id: Some(0),
                target_proto_object_name: None,
                tactic_state: None,
                flags: sim::AttackQueryFlags::empty(),
            },
            |_| true,
        )
        .expect("scenario-layered Mines tactic");
    assert_eq!(mine_profile.action_name(), "ScenarioPlaceMine");
    assert_eq!(mine_profile.mine_object_name(), "scenario_mine");
    assert!((mine_profile.work_range() - 12.0).abs() < f32::EPSILON);

    let minelayer_id = sim::spawn_object_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        1,
        sim::object_prototype_id(&loaded.content.database, "scenario_minelayer").unwrap(),
        glam::Vec3::ZERO,
        glam::Vec3::Z,
    )
    .expect("scenario-layered minelayer prototype");
    assert!(
        (loaded
            .simulation
            .world
            .get_unit(minelayer_id)
            .unwrap()
            .max_hitpoints
            - 80.0)
            .abs()
            < f32::EPSILON
    );
    let minelayer_squad = loaded.simulation.world.create_squad_at(1, glam::Vec3::ZERO);
    assert!(
        loaded
            .simulation
            .world
            .attach_unit_to_squad(minelayer_id, minelayer_squad)
    );
    let mines_command =
        sim::WorkCommand::place_mines_at(1, vec![minelayer_squad], glam::Vec3::ZERO, 0);
    sim::CommandExecutor::with_database(&loaded.content.database).execute(
        &mut loaded.simulation.world,
        &sim::CommandEntry {
            command: sim::QueuedCommand::Work(mines_command),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        },
    );
    loaded
        .simulation
        .world
        .update_entities_with_database_and_gameplay(
            0.05,
            &loaded.content.database,
            &loaded.simulation.gameplay,
        );
    assert!(loaded.simulation.world.units.iter().any(|(_, unit)| {
        unit.proto_object_name == "scenario_mine" && unit.base.player_id == 1
    }));
    assert!(
        (loaded
            .simulation
            .world
            .get_unit(minelayer_id)
            .unwrap()
            .ammunition
            .current()
            - 12.5)
            .abs()
            < f32::EPSILON
    );
}

fn assert_layered_death_spawn_execution(loaded: &mut sim::LoadedGameScenario) {
    let source_position = glam::Vec3::new(30.0, 0.0, 40.0);
    let source_id = sim::spawn_object_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        1,
        sim::object_prototype_id(&loaded.content.database, "scenario_death_source").unwrap(),
        source_position,
        glam::Vec3::X,
    )
    .expect("scenario-layered death source");
    assert!(loaded.simulation.world.kill_unit(source_id, false));

    loaded
        .simulation
        .world
        .update_entities_with_database_and_gameplay(
            0.05,
            &loaded.content.database,
            &loaded.simulation.gameplay,
        );

    assert!(loaded.simulation.world.get_unit(source_id).is_none());
    let spawned = loaded
        .simulation
        .world
        .squads
        .iter()
        .find_map(|(_, squad)| {
            squad
                .proto_squad_name
                .eq_ignore_ascii_case("scenario_death_spawn")
                .then_some(squad)
        })
        .expect("scenario database should drive the death-spawn lifecycle");
    assert_eq!(spawned.base.player_id, 1);
    assert_eq!(spawned.base.position, source_position);
    assert_eq!(spawned.base.forward, glam::Vec3::X);
    assert_eq!(spawned.proto_squad_id, 203);
    assert_eq!(spawned.unit_ids.len(), 2);
    assert!(spawned.unit_ids.iter().all(|unit_id| {
        loaded
            .simulation
            .world
            .get_unit(*unit_id)
            .is_some_and(|unit| {
                unit.proto_object_name
                    .eq_ignore_ascii_case("scenario_death_member")
            })
    }));
}

fn assert_layered_death_replacement_execution(loaded: &mut sim::LoadedGameScenario) {
    let position = glam::Vec3::new(50.0, 0.0, 60.0);
    let source_id = sim::spawn_object_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        1,
        sim::object_prototype_id(&loaded.content.database, "scenario_replacement_source").unwrap(),
        position,
        glam::Vec3::X,
    )
    .expect("scenario-layered replacement source");
    assert!(loaded.simulation.world.kill_unit(source_id, false));

    loaded
        .simulation
        .world
        .update_entities_with_database_and_gameplay(
            0.05,
            &loaded.content.database,
            &loaded.simulation.gameplay,
        );

    let replacement = loaded
        .simulation
        .world
        .get_unit(source_id)
        .expect("death replacement retains the source entity ID");
    assert_eq!(replacement.proto_object_name, "scenario_replacement_target");
    assert_eq!(replacement.proto_object_id, 107);
    assert_eq!(replacement.base.position, position);
    assert_eq!(replacement.base.forward, glam::Vec3::X);
    assert_eq!(replacement.max_hitpoints.to_bits(), 333.0_f32.to_bits());
    assert_eq!(replacement.hitpoints.to_bits(), 333.0_f32.to_bits());
    assert!(replacement.is_static_death_replacement());
    assert_eq!(
        replacement
            .object_state
            .scripted_animation()
            .map(sim::ScriptedAnimation::animation_type),
        Some("Idle")
    );
}

fn assert_layered_database_provenance(loaded: &sim::LoadedGameScenario) {
    for database_path in [
        "data\\gamedata.xml",
        "data\\abilities.xml",
        "data\\civs.xml",
        "data\\leaders.xml",
        "data\\objects.xml",
        "data\\powers.xml",
        "data\\squads.xml",
        "data\\techs.xml",
    ] {
        let provenance = loaded
            .source
            .provenance_data(database_path)
            .expect("database table provenance");
        assert_eq!(provenance.era_label, "layered_test.era");
    }
    for physics_path in [
        "physics\\layered_vehicle.physics",
        "physics\\layered_vehicle.blueprint",
        "physics\\layered_vehicle.shp",
    ] {
        let provenance = loaded
            .source
            .provenance_data(physics_path)
            .expect("physics asset provenance");
        assert_eq!(provenance.era_label, "layered_test.era");
    }
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() <= f32::EPSILON * expected.abs().max(1.0));
}

#[test]
fn scenario_archive_cannot_retroactively_define_startup_veterancy_config() {
    let game_dir = TemporaryGameDir::create();
    write_archive(
        &game_dir.path().join("root.era"),
        &[
            ("data\\gamedata.xml.xmb", BASE_GAME_DATA),
            ("data\\squads.xml.xmb", BASE_SQUADS),
            ("data\\scenariodescriptions.xml.xmb", SCENARIO_DESCRIPTIONS),
            ("startup\\game.cfg", "-Veterancy\n"),
        ],
    );
    write_archive(
        &game_dir.path().join("layered_test.era"),
        &[
            ("data\\gamedata.xml.xmb", SCENARIO_GAME_DATA),
            ("data\\squads.xml.xmb", SCENARIO_SQUADS),
            ("startup\\game.cfg", "Veterancy\n"),
            (
                "scenario\\skirmish\\design\\layered_test\\layered_test.scn.xmb",
                VETERANCY_SCENARIO,
            ),
        ],
    );

    let loaded =
        sim::load_scenario_from_game_dir(&game_dir.path().to_string_lossy(), "layered_test")
            .expect("synthetic scenario should load");

    assert!(!loaded.simulation.world.is_config_defined("Veterancy"));
    assert!(!loaded.simulation.world.veterancy_enabled());
}

fn assert_startup_config_precedes_scenario_archive(loaded: &sim::LoadedGameScenario) {
    let world = &loaded.simulation.world;
    assert!(world.is_config_defined("vEtErAnCy"));
    assert!(world.is_config_defined("baseonlyconfig"));
    assert!(!world.is_config_defined("scenarioonlyconfig"));
    assert!(!world.veterancy_enabled());
}

fn write_archive(path: &Path, files: &[(&str, &str)]) {
    let mut writer = Writer::new();
    for &(game_path, contents) in files {
        let is_config = Path::new(game_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("cfg"));
        let bytes = if is_config {
            contents.as_bytes().to_vec()
        } else {
            pipeline::xmb::Document::from_xml(contents)
                .expect("valid test XML")
                .to_bytes()
                .expect("test XML should encode as XMB")
        };
        writer.add_file(game_path, bytes);
    }
    let output = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .expect("create synthetic ERA");
    writer
        .write_to_encrypted(output, TeaKeys::default_archive_keys())
        .expect("write encrypted synthetic ERA");
}

struct TemporaryGameDir {
    path: PathBuf,
}

impl TemporaryGameDir {
    fn create() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        loop {
            let unique = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "openensemble-scenario-database-{}-{unique}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self { path },
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create temporary game directory: {error}"),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryGameDir {
    fn drop(&mut self) {
        let _ignored = fs::remove_dir_all(&self.path);
    }
}
