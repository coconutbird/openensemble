//! `OpenEnsemble` game client.
//!
//! The client loads a skirmish scenario (`blood_gulch` by default) as a 1v1 game. The local
//! player is the human player of the lobby. The client steps the simulation, draws the scenario
//! with `WorldView` and gives the local player an RTS camera.

mod client;

use anyhow::{Result, bail};
use render::app::WindowConfig;

use client::{Client, ClientOptions};

/// The scenario that the client loads when the command line names none.
const DEFAULT_SCENARIO: &str = "blood_gulch";

/// The command line of the client.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    /// Play the game with these options.
    Play(ClientOptions),
    /// Print the usage text.
    Help,
}

fn parse_cli(args: impl IntoIterator<Item = String>) -> Result<Command> {
    let mut scenario = None;
    let mut seed = 0;
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--seed" => {
                let Some(value) = args.next() else {
                    bail!("--seed requires a value");
                };
                seed = value
                    .parse()
                    .map_err(|error| anyhow::anyhow!("invalid --seed value '{value}': {error}"))?;
            }
            option if option.starts_with('-') => bail!("unknown option '{option}'"),
            _ if scenario.is_some() => bail!("more than one scenario: '{argument}'"),
            _ => scenario = Some(argument),
        }
    }
    Ok(Command::Play(ClientOptions {
        scenario: scenario.unwrap_or_else(|| DEFAULT_SCENARIO.to_owned()),
        seed,
    }))
}

fn print_usage() {
    println!(
        "OpenEnsemble\n\nUsage:\n  openensemble [SCENARIO] [--seed N]\n\nSCENARIO defaults to {DEFAULT_SCENARIO}. Escape quits."
    );
}

fn main() -> Result<()> {
    // Load the `.env` file if it exists. A missing file is not an error.
    if let Err(error) = dotenvy::dotenv_override()
        && !error.not_found()
    {
        return Err(error.into());
    }

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("OpenEnsemble starting...");

    let options = match parse_cli(std::env::args().skip(1))? {
        Command::Play(options) => options,
        Command::Help => {
            print_usage();
            return Ok(());
        }
    };

    let config = WindowConfig::new("OpenEnsemble - Halo Wars", 1280, 720);
    render::app::run(config, Client::new(options))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ClientOptions, Command, DEFAULT_SCENARIO, parse_cli};

    fn parse(args: &[&str]) -> anyhow::Result<Command> {
        parse_cli(args.iter().map(|&argument| argument.to_owned()))
    }

    #[test]
    fn no_arguments_play_blood_gulch_with_seed_zero() {
        assert_eq!(
            parse(&[]).expect("valid command line"),
            Command::Play(ClientOptions {
                scenario: DEFAULT_SCENARIO.to_owned(),
                seed: 0,
            })
        );
    }

    #[test]
    fn a_scenario_and_a_seed_are_accepted() {
        assert_eq!(
            parse(&["tundra", "--seed", "7"]).expect("valid command line"),
            Command::Play(ClientOptions {
                scenario: "tundra".to_owned(),
                seed: 7,
            })
        );
        assert_eq!(parse(&["--help"]).expect("valid"), Command::Help);
    }

    #[test]
    fn invalid_arguments_are_rejected() {
        assert!(parse(&["--seed"]).is_err());
        assert!(parse(&["--seed", "x"]).is_err());
        assert!(parse(&["--fly"]).is_err());
        assert!(parse(&["a", "b"]).is_err());
    }
}
