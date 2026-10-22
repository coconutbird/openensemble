//! Shared fixture helpers for the native power unit tests.

use crate::command_queue::CommandEntry;
use crate::commands::CommandPayload;
use crate::executor::CommandExecutor;
use crate::world::World;
use pipeline::database::hw1::powers::DataEntry;

/// Execute one command from client 1 at time 0 with sequence 0.
pub(super) fn execute_test_command(
    executor: &CommandExecutor<'_>,
    world: &mut World,
    command: CommandPayload,
) {
    executor.execute(
        world,
        &CommandEntry {
            command,
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        },
    );
}

/// Make one power data entry.
pub(super) fn data(data_type: &str, name: &str, value: &str) -> DataEntry {
    DataEntry {
        data_type: data_type.to_owned(),
        name: name.to_owned(),
        value: value.to_owned(),
    }
}

/// Make one `float` power data entry.
pub(super) fn float_data(name: &str, value: f32) -> DataEntry {
    data("float", name, &value.to_string())
}

/// Make one `int` power data entry.
pub(super) fn int_data(name: &str, value: i32) -> DataEntry {
    data("int", name, &value.to_string())
}
