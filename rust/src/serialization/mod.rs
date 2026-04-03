use std::error::Error;
use std::fmt;

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
pub struct CounterSnapshotTextSerializer;

impl Serializer<CounterSnapshot> for CounterSnapshotTextSerializer {
    type Error = SerializationError;

    fn schema_version(&self) -> u32 {
        1
    }

    fn encode(&self, value: &CounterSnapshot) -> Result<Vec<u8>, Self::Error> {
        Ok(format!(
            "v={}\ntick={}\nvalue={}\nvelocity={}\npending_delta={}\npending_entropy={}\nentropy_budget={}\nfinalize_marker={}\n",
            self.schema_version(),
            value.tick,
            value.value,
            value.velocity,
            value.pending_delta,
            value.pending_entropy,
            value.entropy_budget,
            value.finalize_marker
        )
        .into_bytes())
    }

    fn decode(&self, bytes: &[u8]) -> Result<CounterSnapshot, Self::Error> {
        let text = std::str::from_utf8(bytes)
            .map_err(|error| SerializationError(format!("invalid utf8: {error}")))?;

        let mut version = None;
        let mut tick = None;
        let mut value = None;
        let mut velocity = None;
        let mut pending_delta = None;
        let mut pending_entropy = None;
        let mut entropy_budget = None;
        let mut finalize_marker = None;

        for line in text.lines() {
            let (key, raw_value) = line
                .split_once('=')
                .ok_or_else(|| SerializationError(format!("invalid line: {line}")))?;

            match key {
                "v" => version = Some(parse_u32(raw_value)?),
                "tick" => tick = Some(parse_u64(raw_value)?),
                "value" => value = Some(parse_i64(raw_value)?),
                "velocity" => velocity = Some(parse_i64(raw_value)?),
                "pending_delta" => pending_delta = Some(parse_i64(raw_value)?),
                "pending_entropy" => pending_entropy = Some(parse_u64(raw_value)?),
                "entropy_budget" => entropy_budget = Some(parse_u64(raw_value)?),
                "finalize_marker" => finalize_marker = Some(parse_u64(raw_value)?),
                _ => return Err(SerializationError(format!("unexpected field: {key}"))),
            }
        }

        if version != Some(self.schema_version()) {
            return Err(SerializationError(format!(
                "unsupported schema version: {:?}",
                version
            )));
        }

        Ok(CounterSnapshot {
            tick: tick.ok_or_else(|| SerializationError("missing tick".into()))?,
            value: value.ok_or_else(|| SerializationError("missing value".into()))?,
            velocity: velocity.ok_or_else(|| SerializationError("missing velocity".into()))?,
            pending_delta: pending_delta
                .ok_or_else(|| SerializationError("missing pending_delta".into()))?,
            pending_entropy: pending_entropy
                .ok_or_else(|| SerializationError("missing pending_entropy".into()))?,
            entropy_budget: entropy_budget
                .ok_or_else(|| SerializationError("missing entropy_budget".into()))?,
            finalize_marker: finalize_marker
                .ok_or_else(|| SerializationError("missing finalize_marker".into()))?,
        })
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
