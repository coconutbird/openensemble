use era::{TeaKeys, Writer};
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const BASE_GAME_DATA: &str = r"<GameData>
    <DifficultyDefault>0.25</DifficultyDefault>
    <ConstructionDamageMultiplier>0.5</ConstructionDamageMultiplier>
</GameData>";

const SCENARIO_GAME_DATA: &str = r"<GameData>
    <DifficultyDefault>0.82</DifficultyDefault>
    <ConstructionDamageMultiplier>3.0</ConstructionDamageMultiplier>
</GameData>";

const BASE_SQUADS: &str = r#"<Squads>
    <Squad name="base_only" />
</Squads>"#;

const SCENARIO_SQUADS: &str = r#"<Squads>
    <Squad name="scenario_joiner" />
    <Squad name="scenario_target" />
    <MergedSquads>scenario_joiner
        <MergedSquad>scenario_target</MergedSquad>
    </MergedSquads>
</Squads>"#;

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
        <Player Name="Scenario Player" Team="1" Controllable="true" />
    </Players>
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
        ],
    );
    write_archive(
        &game_dir.path().join("layered_test.era"),
        &[
            ("data\\gamedata.xml.xmb", SCENARIO_GAME_DATA),
            ("data\\squads.xml.xmb", SCENARIO_SQUADS),
            (
                "scenario\\skirmish\\design\\layered_test\\layered_test.scn.xmb",
                SCENARIO,
            ),
        ],
    );

    let game_dir_text = game_dir.path().to_string_lossy();
    let loaded = sim::load_scenario_from_game_dir(&game_dir_text, "layered_test")
        .expect("synthetic scenario and database should load together");

    assert!(!loaded.simulation.world.veterancy_enabled());

    let game_data = loaded
        .content
        .database
        .game_data
        .as_ref()
        .expect("scenario game data");
    assert_eq!(game_data.difficulty_default, Some(0.82));
    assert_eq!(game_data.construction_damage_multiplier, Some(3.0));
    assert_eq!(loaded.content.database.squads.len(), 2);
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

    let merged = loaded
        .simulation
        .gameplay
        .merged_squad_profile("scenario_joiner", "scenario_target")
        .expect("scenario-local raw MergedSquads mapping");
    assert_eq!(
        merged.proto_squad_name(),
        "merged_scenario_target_scenario_joiner"
    );
    assert_eq!(merged.proto_squad_id(), 2);

    for database_path in ["data\\gamedata.xml", "data\\squads.xml"] {
        let provenance = loaded
            .source
            .provenance_data(database_path)
            .expect("database table provenance");
        assert_eq!(provenance.era_label, "layered_test.era");
    }
}

fn write_archive(path: &Path, files: &[(&str, &str)]) {
    let mut writer = Writer::new();
    for &(game_path, xml) in files {
        let document = pipeline::xmb::Document::from_xml(xml).expect("valid test XML");
        writer.add_file(
            game_path,
            document.to_bytes().expect("test XML should encode as XMB"),
        );
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
