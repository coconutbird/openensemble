//! Renderer-local adapters for authoritative simulation presentation state.

mod fades;
mod timers;

pub use fades::screen_fade_rgba;
pub use timers::{SimulationTimerAdapter, format_game_timer};
