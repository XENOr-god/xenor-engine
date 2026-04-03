use std::error::Error;
use std::fmt;

use crate::api::CounterCommand;
use crate::canonical::{CANONICAL_TEXT_ENCODING, CanonicalLineReader, CanonicalLineWriter};
use crate::state::CounterSnapshot;

pub trait Serializer<T> {
    type Error;

    fn schema_version(&self) -> u32;
    fn encode(&self, value: &T) -> Result<Vec<u8>, Self::Error>;
    fn decode(&self, bytes: &[u8]) -> Result<T, Self::Error>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializationError(pub String);

impl fmt::Display for SerializationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for SerializationError {}

#[derive(Clone, Debug, Default)]
pub struct CounterCommandTextSerializer;

impl Serializer<CounterCommand> for CounterCommandTextSerializer {
    type Error = SerializationError;

    fn schema_version(&self) -> u32 {
        1
    }

    fn encode(&self, value: &CounterCommand) -> Result<Vec<u8>, Self::Error> {
        let mut writer = CanonicalLineWriter::default();
        writer.push_display("payload_kind", "counter_command");
        writer.push_display("canonical_encoding", CANONICAL_TEXT_ENCODING);
        writer.push_display("command_payload_schema_version", self.schema_version());
        writer.push_display("delta", value.delta);
        writer.push_display("consume_entropy", encode_bool(value.consume_entropy));
        Ok(writer.finish())
    }

    fn decode(&self, bytes: &[u8]) -> Result<CounterCommand, Self::Error> {
        let mut reader = CanonicalLineReader::new(bytes, "counter command payload")
            .map_err(|error| SerializationError(error.to_string()))?;
        reader
            .expect_value("payload_kind", "counter_command", "counter command payload")
            .map_err(|error| SerializationError(error.to_string()))?;
        reader
            .expect_value(
                "canonical_encoding",
                CANONICAL_TEXT_ENCODING,
                "counter command payload",
            )
            .map_err(|error| SerializationError(error.to_string()))?;

        let version = parse_u32(
            reader
                .read_value("command_payload_schema_version", "counter command payload")
                .map_err(|error| SerializationError(error.to_string()))?,
        )?;
        if version != self.schema_version() {
            return Err(SerializationError(format!(
                "unsupported command payload schema version: {version}",
            )));
        }

        let delta = parse_i64(
            reader
                .read_value("delta", "counter command payload")
                .map_err(|error| SerializationError(error.to_string()))?,
        )?;
        let consume_entropy = parse_bool(
            reader
                .read_value("consume_entropy", "counter command payload")
                .map_err(|error| SerializationError(error.to_string()))?,
        )?;

        reader
            .finish("counter command payload")
            .map_err(|error| SerializationError(error.to_string()))?;

        Ok(CounterCommand {
            delta,
            consume_entropy,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct CounterSnapshotTextSerializer;

impl Serializer<CounterSnapshot> for CounterSnapshotTextSerializer {
    type Error = SerializationError;

    fn schema_version(&self) -> u32 {
        1
    }

    fn encode(&self, value: &CounterSnapshot) -> Result<Vec<u8>, Self::Error> {
        let mut writer = CanonicalLineWriter::default();
        writer.push_display("payload_kind", "counter_snapshot");
        writer.push_display("canonical_encoding", CANONICAL_TEXT_ENCODING);
        writer.push_display("snapshot_payload_schema_version", self.schema_version());
        writer.push_display("tick", value.tick);
        writer.push_display("value", value.value);
        writer.push_display("velocity", value.velocity);
        writer.push_display("pending_delta", value.pending_delta);
        writer.push_display("pending_entropy", value.pending_entropy);
        writer.push_display("entropy_budget", value.entropy_budget);
        writer.push_display("finalize_marker", value.finalize_marker);
        Ok(writer.finish())
    }

    fn decode(&self, bytes: &[u8]) -> Result<CounterSnapshot, Self::Error> {
        let mut reader = CanonicalLineReader::new(bytes, "counter snapshot payload")
            .map_err(|error| SerializationError(error.to_string()))?;
        reader
            .expect_value(
                "payload_kind",
                "counter_snapshot",
                "counter snapshot payload",
            )
            .map_err(|error| SerializationError(error.to_string()))?;
        reader
            .expect_value(
                "canonical_encoding",
                CANONICAL_TEXT_ENCODING,
                "counter snapshot payload",
            )
            .map_err(|error| SerializationError(error.to_string()))?;

        let version = parse_u32(
            reader
                .read_value(
                    "snapshot_payload_schema_version",
                    "counter snapshot payload",
                )
                .map_err(|error| SerializationError(error.to_string()))?,
        )?;
        if version != self.schema_version() {
            return Err(SerializationError(format!(
                "unsupported snapshot payload schema version: {version}",
            )));
        }

        let tick = parse_u64(
            reader
                .read_value("tick", "counter snapshot payload")
                .map_err(|error| SerializationError(error.to_string()))?,
        )?;
        let value = parse_i64(
            reader
                .read_value("value", "counter snapshot payload")
                .map_err(|error| SerializationError(error.to_string()))?,
        )?;
        let velocity = parse_i64(
            reader
                .read_value("velocity", "counter snapshot payload")
                .map_err(|error| SerializationError(error.to_string()))?,
        )?;
        let pending_delta = parse_i64(
            reader
                .read_value("pending_delta", "counter snapshot payload")
                .map_err(|error| SerializationError(error.to_string()))?,
        )?;
        let pending_entropy = parse_u64(
            reader
                .read_value("pending_entropy", "counter snapshot payload")
                .map_err(|error| SerializationError(error.to_string()))?,
        )?;
        let entropy_budget = parse_u64(
            reader
                .read_value("entropy_budget", "counter snapshot payload")
                .map_err(|error| SerializationError(error.to_string()))?,
        )?;
        let finalize_marker = parse_u64(
            reader
                .read_value("finalize_marker", "counter snapshot payload")
                .map_err(|error| SerializationError(error.to_string()))?,
        )?;

        reader
            .finish("counter snapshot payload")
            .map_err(|error| SerializationError(error.to_string()))?;

        Ok(CounterSnapshot {
            tick,
            value,
            velocity,
            pending_delta,
            pending_entropy,
            entropy_budget,
            finalize_marker,
        })
    }
}

fn encode_bool(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn parse_bool(value: &str) -> Result<bool, SerializationError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(SerializationError(format!("invalid bool `{value}`"))),
    }
}

fn parse_u32(value: &str) -> Result<u32, SerializationError> {
    value
        .parse()
        .map_err(|error| SerializationError(format!("invalid u32 `{value}`: {error}")))
}

fn parse_u64(value: &str) -> Result<u64, SerializationError> {
    value
        .parse()
        .map_err(|error| SerializationError(format!("invalid u64 `{value}`: {error}")))
}

fn parse_i64(value: &str) -> Result<i64, SerializationError> {
    value
        .parse()
        .map_err(|error| SerializationError(format!("invalid i64 `{value}`: {error}")))
}
