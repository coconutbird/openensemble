//! Time synchronization for deterministic lockstep networking.
//!
//! This module implements the timing system that keeps all clients
//! synchronized during multiplayer games.

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{self, Read, Write};

/// Constants matching vanilla BTimeSync.
pub mod constants {
    /// Default update interval in milliseconds.
    pub const DEFAULT_UPDATE_INTERVAL: u32 = 100;
    /// Default timeout for game start.
    pub const DEFAULT_TIMEOUT_VALUE: u32 = 10000;
    /// Initial send time offset.
    pub const INITIAL_SEND_TIME: u32 = 1000;
    /// Maximum update interval.
    pub const MAX_UPDATE_INTERVAL: u32 = 5000;
    /// Minimum service interval.
    pub const MIN_SERVICE_INTERVAL: u32 = 10;
    /// Divisor for ping-based update interval.
    pub const UPDATE_INTERVAL_PING_DIVISOR: u32 = 4;
    /// Minimum frequency for sending time markers.
    pub const MINIMUM_SEND_FREQUENCY: u32 = 200;
    /// Constant update interval used before variable timing kicks in.
    pub const CONSTANT_UPDATE_INTERVAL: u32 = 100;
    /// Maximum clients supported.
    pub const MAX_CLIENTS: usize = 16;
}

/// Timing record sent between clients.
#[derive(Debug, Clone, Copy, Default)]
pub struct TimingRecord {
    /// The send time this record was generated at.
    pub send_time: u32,
    /// The timing value (update interval).
    pub timing: u8,
}

impl TimingRecord {
    pub fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_u8(self.timing)?;
        writer.write_u32::<LittleEndian>(self.send_time)?;
        Ok(())
    }

    pub fn deserialize<R: Read>(reader: &mut R) -> io::Result<Self> {
        Ok(Self {
            timing: reader.read_u8()?,
            send_time: reader.read_u32::<LittleEndian>()?,
        })
    }
}

/// Client time history for tracking timing data.
#[derive(Debug, Clone)]
pub struct ClientTimeHistory {
    /// Ring buffer of client times.
    times: [u32; 32],
    /// Last recorded time.
    pub last_time: u32,
    /// Earliest time in the buffer.
    pub earliest_time: u32,
    /// Insert index for ring buffer.
    insert_index: u32,
    /// Earliest index in ring buffer.
    earliest_index: u32,
}

impl Default for ClientTimeHistory {
    fn default() -> Self {
        Self {
            times: [0; 32],
            last_time: 0,
            earliest_time: 0,
            insert_index: 0,
            earliest_index: 0,
        }
    }
}

impl ClientTimeHistory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a time interval to the history.
    pub fn add_interval(&mut self, interval: u8) {
        self.add_time(self.last_time + interval as u32);
    }

    /// Add an absolute time to the history.
    pub fn add_time(&mut self, time: u32) {
        let idx = (self.insert_index as usize) % self.times.len();
        self.times[idx] = time;
        self.last_time = time;
        self.insert_index = self.insert_index.wrapping_add(1);

        // Update earliest if needed
        if self.earliest_time == 0 || time < self.earliest_time {
            self.earliest_time = time;
            self.earliest_index = idx as u32;
        }
    }

    /// Reset the history.
    pub fn reset(&mut self, time: u32) {
        self.times = [0; 32];
        self.last_time = time;
        self.earliest_time = time;
        self.insert_index = 0;
        self.earliest_index = 0;
    }

    /// Advance time by an amount.
    pub fn advance(&mut self, amount: u32) {
        self.last_time = self.last_time.saturating_add(amount);
        self.earliest_time = self.earliest_time.saturating_add(amount);
    }
}

/// Time synchronization state for a multiplayer session.
#[derive(Debug, Clone)]
pub struct TimeSync {
    /// Current receive time (safe to process commands up to this time).
    pub recv_time: u32,
    /// Current send time (time marker for outgoing commands).
    pub send_time: u32,
    /// Send time offset.
    pub send_offset: u32,
    /// Earliest allowed receive time.
    pub earliest_allowed_recv_time: u32,
    /// Current update interval.
    pub recv_update_interval: u32,
    /// Send update interval.
    pub send_update_interval: u32,
    /// Ping approximation.
    pub ping_approximation: u32,
    /// Whether time is rolling (game has started).
    pub time_rolling: bool,
    /// Client timing histories.
    pub client_histories: [ClientTimeHistory; constants::MAX_CLIENTS],
}

impl Default for TimeSync {
    fn default() -> Self {
        Self {
            recv_time: 0,
            send_time: constants::INITIAL_SEND_TIME,
            send_offset: constants::INITIAL_SEND_TIME,
            earliest_allowed_recv_time: 0,
            recv_update_interval: constants::CONSTANT_UPDATE_INTERVAL,
            send_update_interval: constants::CONSTANT_UPDATE_INTERVAL,
            ping_approximation: 0,
            time_rolling: false,
            client_histories: std::array::from_fn(|_| ClientTimeHistory::default()),
        }
    }
}
