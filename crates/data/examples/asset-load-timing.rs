//! Time the asset-loading stages used by `OpenEnsemble`.

use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let runtime = arguments.iter().any(|argument| argument == "--runtime");
    let scenario = arguments
        .iter()
        .find(|argument| !argument.starts_with('-'))
        .cloned()
        .unwrap_or_else(|| "blood_gulch".to_owned());
    let game_dir = data::game_dir().to_string_lossy().into_owned();
    let total_started = Instant::now();

    let started = Instant::now();
    let mut source = data::loader::load_game_dir(&game_dir);
    let archives = source.era_count();
    let archive_elapsed = started.elapsed();

    let started = Instant::now();
    if !source.load_scenario(&scenario) {
        return Err(format!("scenario archive for '{scenario}' was not found").into());
    }
    let scenario_elapsed = started.elapsed();

    let started = Instant::now();
    let options = if runtime {
        data::pipeline::hw1::WorldLoadOptions::runtime()
    } else {
        data::pipeline::hw1::WorldLoadOptions::full()
    };
    let mut world =
        data::pipeline::hw1::World::load_from_source_with_options(&mut source, options)?;
    let world_elapsed = started.elapsed();

    let started = Instant::now();
    let active_visuals = if runtime {
        let names = world
            .scenario_data
            .as_ref()
            .into_iter()
            .flat_map(data::ScenarioData::objects)
            .map(|object| object.proto_name.clone())
            .collect::<Vec<_>>();
        world.load_visuals_for(&mut source, names)
    } else {
        0
    };
    let visual_elapsed = started.elapsed();

    println!(
        "archive stack:   {:>9.3} ms ({archives} archives)",
        archive_elapsed.as_secs_f64() * 1_000.0
    );
    println!(
        "scenario layer:  {:>9.3} ms",
        scenario_elapsed.as_secs_f64() * 1_000.0
    );
    println!(
        "world assets:    {:>9.3} ms ({} objects, {} visuals, {} profile)",
        world_elapsed.as_secs_f64() * 1_000.0,
        world.database.objects.len(),
        world.visuals.len(),
        if runtime { "runtime" } else { "full" }
    );
    if runtime {
        println!(
            "active visuals: {:>9.3} ms ({active_visuals} loaded)",
            visual_elapsed.as_secs_f64() * 1_000.0
        );
    }
    println!(
        "total:           {:>9.3} ms",
        total_started.elapsed().as_secs_f64() * 1_000.0
    );

    Ok(())
}
