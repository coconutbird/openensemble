//! Renderer-local adapters for authoritative simulation presentation state.

mod fades;
mod rumbles;
mod timers;

pub use fades::screen_fade_rgba;
pub use rumbles::{RumbleProjection, SimulationRumbleAdapter};
pub use timers::{SimulationTimerAdapter, format_game_timer};
