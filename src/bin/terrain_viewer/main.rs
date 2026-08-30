//! Terrain viewer for Halo Wars XTD/XTT data.
//!
//! Uses packed XTD vertex textures, the GPU XTT compositor, terrain-conforming
//! roads, and foliage reconstructed from the XTT index buffers.
//! WASD + mouse to fly around the terrain.
//! Display mode 0 is the retail-style lit terrain view.

mod camera;
mod capture;
mod dynamic_alpha;
mod foliage;
mod gpu;
mod resources;
mod roads;
mod shadow;
mod types;
mod viewer;

use anyhow::{Context, Result, bail};
use glam::Vec2;
use std::path::PathBuf;

use capture::{CaptureConfig, DEFAULT_CAPTURE_SIZE};
use viewer::TerrainViewer;
use xcore::app::WindowConfig;

#[derive(Debug)]
struct CliOptions {
    source: String,
    capture: Option<CaptureConfig>,
    show_help: bool,
}

fn parse_cli(args: impl IntoIterator<Item = String>) -> Result<CliOptions> {
    let mut source = None;
    let mut capture_path = None;
    let mut capture_size = DEFAULT_CAPTURE_SIZE;
    let mut capture_mode = 0;
    let mut capture_center = None;
    let mut capture_span = None;
    let mut show_help = false;
    let mut args = args.into_iter();

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--capture-top-down" => {
                let path = args
                    .next()
                    .context("--capture-top-down requires a PNG path")?;
                capture_path = Some(PathBuf::from(path));
            }
            "--capture-size" => {
                let value = args
                    .next()
                    .context("--capture-size requires a pixel count")?;
                capture_size = value
                    .parse::<u32>()
                    .with_context(|| format!("invalid capture size '{value}'"))?;
                if capture_size == 0 {
                    bail!("capture size must be greater than zero");
                }
            }
            "--capture-mode" => {
                let value = args
                    .next()
                    .context("--capture-mode requires a mode number")?;
                capture_mode = value
                    .parse::<u32>()
                    .with_context(|| format!("invalid capture mode '{value}'"))?;
                if capture_mode > 15 {
                    bail!("capture mode must be between 0 and 15");
                }
            }
            "--capture-center" => {
                let x = args.next().context("--capture-center requires X and Z")?;
                let z = args.next().context("--capture-center requires X and Z")?;
                let x = x
                    .parse::<f32>()
                    .with_context(|| format!("invalid capture center X '{x}'"))?;
                let z = z
                    .parse::<f32>()
                    .with_context(|| format!("invalid capture center Z '{z}'"))?;
                if !x.is_finite() || !z.is_finite() {
                    bail!("capture center must be finite");
                }
                capture_center = Some(Vec2::new(x, z));
            }
            "--capture-span" => {
                let value = args
                    .next()
                    .context("--capture-span requires a world-space size")?;
                let span = value
                    .parse::<f32>()
                    .with_context(|| format!("invalid capture span '{value}'"))?;
                if !span.is_finite() || span <= 0.0 {
                    bail!("capture span must be a finite value greater than zero");
                }
                capture_span = Some(span);
            }
            "--help" | "-h" => show_help = true,
            _ if argument.starts_with('-') => bail!("unknown option '{argument}'"),
            _ => {
                if source.replace(argument).is_some() {
                    bail!("only one scenario name or XTD path may be supplied");
                }
            }
        }
    }
    if capture_path.is_none() && capture_size != DEFAULT_CAPTURE_SIZE {
        bail!("--capture-size requires --capture-top-down");
    }
    if capture_path.is_none() && capture_mode != 0 {
        bail!("--capture-mode requires --capture-top-down");
    }
    if capture_path.is_none() && (capture_center.is_some() || capture_span.is_some()) {
        bail!("--capture-center and --capture-span require --capture-top-down");
    }
    if capture_center.is_some() != capture_span.is_some() {
        bail!("--capture-center and --capture-span must be supplied together");
    }

    let capture = capture_path.map(|path| {
        let config = CaptureConfig::new(path, capture_size).with_debug_mode(capture_mode);
        match (capture_center, capture_span) {
            (Some(center), Some(span)) => config.with_region(center, span),
            _ => config,
        }
    });

    Ok(CliOptions {
        source: source.unwrap_or_else(|| "blood_gulch".to_string()),
        capture,
        show_help,
    })
}

fn print_usage() {
    println!(
        "Terrain Viewer\n\nUsage:\n  terrain_viewer [SCENARIO|FILE.xtd]\n  terrain_viewer [SCENARIO|FILE.xtd] --capture-top-down OUTPUT.png [--capture-size PIXELS] [--capture-mode 0..15] [--capture-center X Z --capture-span WORLD_UNITS]"
    );
}

fn main() -> Result<()> {
    // Load .env file if present (ignore errors if not found)
    dotenvy::dotenv_override()?;

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Terrain Viewer starting...");

    let options = parse_cli(std::env::args().skip(1))?;
    if options.show_help {
        print_usage();
        return Ok(());
    }

    let mut viewer = if std::path::Path::new(&options.source)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xtd"))
    {
        log::info!("Loading terrain from file: {}", options.source);
        TerrainViewer::new(Some(PathBuf::from(&options.source)))
    } else {
        log::info!("Loading scenario: {}", options.source);
        TerrainViewer::from_scenario(options.source)
    };

    let capture_size = options.capture.as_ref().map(|capture| capture.size);
    if let Some(capture) = options.capture {
        log::info!(
            "Capturing deterministic top-down mode {} view to {}",
            capture.debug_mode,
            capture.output_path.display()
        );
        viewer = viewer.with_capture(capture);
    }

    let (width, height) = capture_size.map_or((1280, 720), |size| (size, size));
    let mut config = WindowConfig::new("Terrain Viewer - Halo Wars XTD", width, height);
    if capture_size.is_some() {
        config.resizable = false;
        config.vsync = false;
    }
    render::run_3d(config, viewer)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_CAPTURE_SIZE, parse_cli};
    use glam::Vec2;
    use std::path::Path;

    #[test]
    fn capture_cli_is_order_independent() {
        let options = parse_cli([
            "--capture-top-down".to_string(),
            "out.png".to_string(),
            "blood_gulch".to_string(),
            "--capture-size".to_string(),
            "1024".to_string(),
        ])
        .expect("valid capture arguments");
        assert_eq!(options.source, "blood_gulch");
        let capture = options.capture.expect("capture must be configured");
        assert_eq!(capture.output_path, Path::new("out.png"));
        assert_eq!(capture.size, 1024);
        assert_eq!(capture.debug_mode, 0);
    }

    #[test]
    fn cli_defaults_to_blood_gulch() {
        let options = parse_cli([]).expect("empty arguments are valid");
        assert_eq!(options.source, "blood_gulch");
        assert!(options.capture.is_none());
        assert_eq!(DEFAULT_CAPTURE_SIZE, 2048);
    }

    #[test]
    fn capture_cli_accepts_a_focused_world_region() {
        let options = parse_cli([
            "--capture-top-down".to_string(),
            "rock.png".to_string(),
            "--capture-center".to_string(),
            "780".to_string(),
            "880".to_string(),
            "--capture-span".to_string(),
            "192".to_string(),
        ])
        .expect("valid focused capture arguments");
        let capture = options.capture.expect("capture must be configured");
        assert_eq!(capture.center, Some(Vec2::new(780.0, 880.0)));
        assert_eq!(capture.span, Some(192.0));
    }

    #[test]
    fn capture_cli_accepts_an_explicit_debug_mode() {
        let options = parse_cli([
            "--capture-top-down".to_string(),
            "lit.png".to_string(),
            "--capture-mode".to_string(),
            "8".to_string(),
        ])
        .expect("valid lit capture arguments");
        assert_eq!(
            options
                .capture
                .expect("capture must be configured")
                .debug_mode,
            8
        );
    }
}
