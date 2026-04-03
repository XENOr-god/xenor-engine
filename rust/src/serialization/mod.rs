use std::error::Error;
use std::fmt;

use crate::api::CounterCommand;
use crate::canonical::{CANONICAL_TEXT_ENCODING, CanonicalLineReader, CanonicalLineWriter};
use crate::config::CounterSimulationConfig;
use crate::deterministic::DeterministicList;
use crate::engine::SnapshotPolicy;
use crate::settlement::{
    SettlementCommand, SettlementSimulationConfig, SettlementSnapshot, SettlementStatus,
    WorkerAllocation,
};
use crate::state::{CounterEntityInit, CounterEntitySnapshot, CounterSnapshot, EntityId};
use crate::validation::ValidationPolicy;

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
        2
    }

    fn encode(&self, value: &CounterSnapshot) -> Result<Vec<u8>, Self::Error> {
        value.validate().map_err(SerializationError)?;

        let mut writer = CanonicalLineWriter::default();
        writer.push_display("payload_kind", "counter_snapshot");
        writer.push_display("canonical_encoding", CANONICAL_TEXT_ENCODING);
        writer.push_display("snapshot_payload_schema_version", self.schema_version());
        writer.push_display("tick", value.tick);
        writer.push_display("next_entity_id", value.next_entity_id);
        writer.push_display("primary_entity_id", value.primary_entity.0);
        writer.push_display("entity_count", value.entities.len());
        for (index, entity) in value.entities.iter().enumerate() {
            writer.push_display(format!("entity.{index}.id").as_str(), entity.id.0);
            writer.push_display(format!("entity.{index}.value").as_str(), entity.value);
            writer.push_display(format!("entity.{index}.velocity").as_str(), entity.velocity);
        }
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
        let next_entity_id = parse_u64(
            reader
                .read_value("next_entity_id", "counter snapshot payload")
                .map_err(|error| SerializationError(error.to_string()))?,
        )?;
        let primary_entity = EntityId(parse_u64(
            reader
                .read_value("primary_entity_id", "counter snapshot payload")
                .map_err(|error| SerializationError(error.to_string()))?,
        )?);
        let entity_count = parse_usize(
            reader
                .read_value("entity_count", "counter snapshot payload")
                .map_err(|error| SerializationError(error.to_string()))?,
        )?;
        let mut entities = DeterministicList::new();
        for index in 0..entity_count {
            entities.push(CounterEntitySnapshot {
                id: EntityId(parse_u64(
                    reader
                        .read_value(
                            format!("entity.{index}.id").as_str(),
                            "counter snapshot payload",
                        )
                        .map_err(|error| SerializationError(error.to_string()))?,
                )?),
                value: parse_i64(
                    reader
                        .read_value(
                            format!("entity.{index}.value").as_str(),
                            "counter snapshot payload",
                        )
                        .map_err(|error| SerializationError(error.to_string()))?,
                )?,
                velocity: parse_i64(
                    reader
                        .read_value(
                            format!("entity.{index}.velocity").as_str(),
                            "counter snapshot payload",
                        )
                        .map_err(|error| SerializationError(error.to_string()))?,
                )?,
            });
        }
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

        let snapshot = CounterSnapshot {
            tick,
            next_entity_id,
            primary_entity,
            entities,
            pending_delta,
            pending_entropy,
            entropy_budget,
            finalize_marker,
        };
        snapshot.validate().map_err(SerializationError)?;

        Ok(snapshot)
    }
}

#[derive(Clone, Debug, Default)]
pub struct CounterConfigTextSerializer;

impl Serializer<CounterSimulationConfig> for CounterConfigTextSerializer {
    type Error = SerializationError;

    fn schema_version(&self) -> u32 {
        2
    }

    fn encode(&self, value: &CounterSimulationConfig) -> Result<Vec<u8>, Self::Error> {
        value
            .validate()
            .map_err(|error| SerializationError(error.to_string()))?;

        let mut writer = CanonicalLineWriter::default();
        writer.push_display("payload_kind", "counter_config");
        writer.push_display("canonical_encoding", CANONICAL_TEXT_ENCODING);
        writer.push_display("config_payload_schema_version", self.schema_version());
        writer.push_display("initial_value", value.initial_value);
        writer.push_display("initial_velocity", value.initial_velocity);
        writer.push_display("initial_entity_count", value.initial_entities.len());
        for (index, entity) in value.initial_entities.iter().enumerate() {
            writer.push_display(
                format!("initial_entity.{index}.value").as_str(),
                entity.value,
            );
            writer.push_display(
                format!("initial_entity.{index}.velocity").as_str(),
                entity.velocity,
            );
        }
        writer.push_display("snapshot_policy", value.snapshot_policy.canonical_string());
        writer.push_display("validation_policy", value.validation_policy.as_str());
        writer.push_display("max_abs_value", value.max_abs_value);
        writer.push_display("max_abs_velocity", value.max_abs_velocity);
        writer.push_display("max_abs_pending_delta", value.max_abs_pending_delta);
        Ok(writer.finish())
    }

    fn decode(&self, bytes: &[u8]) -> Result<CounterSimulationConfig, Self::Error> {
        let mut reader = CanonicalLineReader::new(bytes, "counter config payload")
            .map_err(|error| SerializationError(error.to_string()))?;
        reader
            .expect_value("payload_kind", "counter_config", "counter config payload")
            .map_err(|error| SerializationError(error.to_string()))?;
        reader
            .expect_value(
                "canonical_encoding",
                CANONICAL_TEXT_ENCODING,
                "counter config payload",
            )
            .map_err(|error| SerializationError(error.to_string()))?;

        let version = parse_u32(
            reader
                .read_value("config_payload_schema_version", "counter config payload")
                .map_err(|error| SerializationError(error.to_string()))?,
        )?;
        if version != self.schema_version() {
            return Err(SerializationError(format!(
                "unsupported config payload schema version: {version}",
            )));
        }

        let config = CounterSimulationConfig {
            initial_value: parse_i64(
                reader
                    .read_value("initial_value", "counter config payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            initial_velocity: parse_i64(
                reader
                    .read_value("initial_velocity", "counter config payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            initial_entities: {
                let entity_count = parse_usize(
                    reader
                        .read_value("initial_entity_count", "counter config payload")
                        .map_err(|error| SerializationError(error.to_string()))?,
                )?;
                let mut entities = DeterministicList::new();
                for index in 0..entity_count {
                    entities.push(CounterEntityInit {
                        value: parse_i64(
                            reader
                                .read_value(
                                    format!("initial_entity.{index}.value").as_str(),
                                    "counter config payload",
                                )
                                .map_err(|error| SerializationError(error.to_string()))?,
                        )?,
                        velocity: parse_i64(
                            reader
                                .read_value(
                                    format!("initial_entity.{index}.velocity").as_str(),
                                    "counter config payload",
                                )
                                .map_err(|error| SerializationError(error.to_string()))?,
                        )?,
                    });
                }
                entities
            },
            snapshot_policy: parse_snapshot_policy(
                reader
                    .read_value("snapshot_policy", "counter config payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            validation_policy: parse_validation_policy(
                reader
                    .read_value("validation_policy", "counter config payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            max_abs_value: parse_i64(
                reader
                    .read_value("max_abs_value", "counter config payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            max_abs_velocity: parse_i64(
                reader
                    .read_value("max_abs_velocity", "counter config payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            max_abs_pending_delta: parse_i64(
                reader
                    .read_value("max_abs_pending_delta", "counter config payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
        };

        config
            .validate()
            .map_err(|error| SerializationError(error.to_string()))?;

        reader
            .finish("counter config payload")
            .map_err(|error| SerializationError(error.to_string()))?;

        Ok(config)
    }
}

#[derive(Clone, Debug, Default)]
pub struct SettlementCommandTextSerializer;

impl Serializer<SettlementCommand> for SettlementCommandTextSerializer {
    type Error = SerializationError;

    fn schema_version(&self) -> u32 {
        1
    }

    fn encode(&self, value: &SettlementCommand) -> Result<Vec<u8>, Self::Error> {
        let (opcode, allocation) = match value {
            SettlementCommand::Hold => ("hold", WorkerAllocation::zero()),
            SettlementCommand::SetWorkerAllocation(allocation) => {
                ("set_worker_allocation", *allocation)
            }
        };

        let mut writer = CanonicalLineWriter::default();
        writer.push_display("payload_kind", "settlement_command");
        writer.push_display("canonical_encoding", CANONICAL_TEXT_ENCODING);
        writer.push_display("command_payload_schema_version", self.schema_version());
        writer.push_display("opcode", opcode);
        writer.push_display("allocation_farmers", allocation.farmers);
        writer.push_display("allocation_loggers", allocation.loggers);
        Ok(writer.finish())
    }

    fn decode(&self, bytes: &[u8]) -> Result<SettlementCommand, Self::Error> {
        let mut reader = CanonicalLineReader::new(bytes, "settlement command payload")
            .map_err(|error| SerializationError(error.to_string()))?;
        reader
            .expect_value(
                "payload_kind",
                "settlement_command",
                "settlement command payload",
            )
            .map_err(|error| SerializationError(error.to_string()))?;
        reader
            .expect_value(
                "canonical_encoding",
                CANONICAL_TEXT_ENCODING,
                "settlement command payload",
            )
            .map_err(|error| SerializationError(error.to_string()))?;

        let version = parse_u32(
            reader
                .read_value(
                    "command_payload_schema_version",
                    "settlement command payload",
                )
                .map_err(|error| SerializationError(error.to_string()))?,
        )?;
        if version != self.schema_version() {
            return Err(SerializationError(format!(
                "unsupported command payload schema version: {version}",
            )));
        }

        let opcode = reader
            .read_value("opcode", "settlement command payload")
            .map_err(|error| SerializationError(error.to_string()))?;
        let allocation = WorkerAllocation {
            farmers: parse_u32(
                reader
                    .read_value("allocation_farmers", "settlement command payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            loggers: parse_u32(
                reader
                    .read_value("allocation_loggers", "settlement command payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
        };

        reader
            .finish("settlement command payload")
            .map_err(|error| SerializationError(error.to_string()))?;

        match opcode {
            "hold" => {
                if allocation != WorkerAllocation::zero() {
                    return Err(SerializationError(format!(
                        "hold command must encode zero allocation, got farmers={}, loggers={}",
                        allocation.farmers, allocation.loggers
                    )));
                }

                Ok(SettlementCommand::Hold)
            }
            "set_worker_allocation" => Ok(SettlementCommand::SetWorkerAllocation(allocation)),
            _ => Err(SerializationError(format!(
                "invalid settlement opcode `{opcode}`"
            ))),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SettlementSnapshotTextSerializer;

impl Serializer<SettlementSnapshot> for SettlementSnapshotTextSerializer {
    type Error = SerializationError;

    fn schema_version(&self) -> u32 {
        1
    }

    fn encode(&self, value: &SettlementSnapshot) -> Result<Vec<u8>, Self::Error> {
        value.validate().map_err(SerializationError)?;

        let mut writer = CanonicalLineWriter::default();
        writer.push_display("payload_kind", "settlement_snapshot");
        writer.push_display("canonical_encoding", CANONICAL_TEXT_ENCODING);
        writer.push_display("snapshot_payload_schema_version", self.schema_version());
        writer.push_display("tick", value.tick);
        writer.push_display("population", value.population);
        writer.push_display(
            "current_allocation_farmers",
            value.current_allocation.farmers,
        );
        writer.push_display(
            "current_allocation_loggers",
            value.current_allocation.loggers,
        );
        writer.push_display(
            "pending_allocation_present",
            encode_bool(value.pending_allocation_present),
        );
        writer.push_display(
            "pending_allocation_farmers",
            value.pending_allocation.farmers,
        );
        writer.push_display(
            "pending_allocation_loggers",
            value.pending_allocation.loggers,
        );
        writer.push_display("food", value.food);
        writer.push_display("wood", value.wood);
        writer.push_display("last_produced_food", value.last_produced_food);
        writer.push_display("last_produced_wood", value.last_produced_wood);
        writer.push_display("last_consumed_food", value.last_consumed_food);
        writer.push_display("last_consumed_wood", value.last_consumed_wood);
        writer.push_display("last_food_shortage", value.last_food_shortage);
        writer.push_display("last_wood_shortage", value.last_wood_shortage);
        writer.push_display("total_food_produced", value.total_food_produced);
        writer.push_display("total_wood_produced", value.total_wood_produced);
        writer.push_display("total_food_consumed", value.total_food_consumed);
        writer.push_display("total_wood_consumed", value.total_wood_consumed);
        writer.push_display("total_food_shortage", value.total_food_shortage);
        writer.push_display("total_wood_shortage", value.total_wood_shortage);
        writer.push_display("shortage_ticks", value.shortage_ticks);
        writer.push_display("last_status", value.last_status.as_str());
        writer.push_display("finalize_marker", value.finalize_marker);
        Ok(writer.finish())
    }

    fn decode(&self, bytes: &[u8]) -> Result<SettlementSnapshot, Self::Error> {
        let mut reader = CanonicalLineReader::new(bytes, "settlement snapshot payload")
            .map_err(|error| SerializationError(error.to_string()))?;
        reader
            .expect_value(
                "payload_kind",
                "settlement_snapshot",
                "settlement snapshot payload",
            )
            .map_err(|error| SerializationError(error.to_string()))?;
        reader
            .expect_value(
                "canonical_encoding",
                CANONICAL_TEXT_ENCODING,
                "settlement snapshot payload",
            )
            .map_err(|error| SerializationError(error.to_string()))?;

        let version = parse_u32(
            reader
                .read_value(
                    "snapshot_payload_schema_version",
                    "settlement snapshot payload",
                )
                .map_err(|error| SerializationError(error.to_string()))?,
        )?;
        if version != self.schema_version() {
            return Err(SerializationError(format!(
                "unsupported snapshot payload schema version: {version}",
            )));
        }

        let snapshot = SettlementSnapshot {
            tick: parse_u64(
                reader
                    .read_value("tick", "settlement snapshot payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            population: parse_u32(
                reader
                    .read_value("population", "settlement snapshot payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            current_allocation: WorkerAllocation {
                farmers: parse_u32(
                    reader
                        .read_value("current_allocation_farmers", "settlement snapshot payload")
                        .map_err(|error| SerializationError(error.to_string()))?,
                )?,
                loggers: parse_u32(
                    reader
                        .read_value("current_allocation_loggers", "settlement snapshot payload")
                        .map_err(|error| SerializationError(error.to_string()))?,
                )?,
            },
            pending_allocation_present: parse_bool(
                reader
                    .read_value("pending_allocation_present", "settlement snapshot payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            pending_allocation: WorkerAllocation {
                farmers: parse_u32(
                    reader
                        .read_value("pending_allocation_farmers", "settlement snapshot payload")
                        .map_err(|error| SerializationError(error.to_string()))?,
                )?,
                loggers: parse_u32(
                    reader
                        .read_value("pending_allocation_loggers", "settlement snapshot payload")
                        .map_err(|error| SerializationError(error.to_string()))?,
                )?,
            },
            food: parse_i64(
                reader
                    .read_value("food", "settlement snapshot payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            wood: parse_i64(
                reader
                    .read_value("wood", "settlement snapshot payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            last_produced_food: parse_i64(
                reader
                    .read_value("last_produced_food", "settlement snapshot payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            last_produced_wood: parse_i64(
                reader
                    .read_value("last_produced_wood", "settlement snapshot payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            last_consumed_food: parse_i64(
                reader
                    .read_value("last_consumed_food", "settlement snapshot payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            last_consumed_wood: parse_i64(
                reader
                    .read_value("last_consumed_wood", "settlement snapshot payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            last_food_shortage: parse_i64(
                reader
                    .read_value("last_food_shortage", "settlement snapshot payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            last_wood_shortage: parse_i64(
                reader
                    .read_value("last_wood_shortage", "settlement snapshot payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            total_food_produced: parse_i64(
                reader
                    .read_value("total_food_produced", "settlement snapshot payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            total_wood_produced: parse_i64(
                reader
                    .read_value("total_wood_produced", "settlement snapshot payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            total_food_consumed: parse_i64(
                reader
                    .read_value("total_food_consumed", "settlement snapshot payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            total_wood_consumed: parse_i64(
                reader
                    .read_value("total_wood_consumed", "settlement snapshot payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            total_food_shortage: parse_i64(
                reader
                    .read_value("total_food_shortage", "settlement snapshot payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            total_wood_shortage: parse_i64(
                reader
                    .read_value("total_wood_shortage", "settlement snapshot payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            shortage_ticks: parse_u64(
                reader
                    .read_value("shortage_ticks", "settlement snapshot payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            last_status: parse_settlement_status(
                reader
                    .read_value("last_status", "settlement snapshot payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            finalize_marker: parse_u64(
                reader
                    .read_value("finalize_marker", "settlement snapshot payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
        };

        reader
            .finish("settlement snapshot payload")
            .map_err(|error| SerializationError(error.to_string()))?;

        snapshot.validate().map_err(SerializationError)?;
        Ok(snapshot)
    }
}

#[derive(Clone, Debug, Default)]
pub struct SettlementConfigTextSerializer;

impl Serializer<SettlementSimulationConfig> for SettlementConfigTextSerializer {
    type Error = SerializationError;

    fn schema_version(&self) -> u32 {
        1
    }

    fn encode(&self, value: &SettlementSimulationConfig) -> Result<Vec<u8>, Self::Error> {
        value
            .validate()
            .map_err(|error| SerializationError(error.to_string()))?;

        let mut writer = CanonicalLineWriter::default();
        writer.push_display("payload_kind", "settlement_config");
        writer.push_display("canonical_encoding", CANONICAL_TEXT_ENCODING);
        writer.push_display("config_payload_schema_version", self.schema_version());
        writer.push_display("population", value.population);
        writer.push_display("initial_food", value.initial_food);
        writer.push_display("initial_wood", value.initial_wood);
        writer.push_display(
            "initial_allocation_farmers",
            value.initial_allocation.farmers,
        );
        writer.push_display(
            "initial_allocation_loggers",
            value.initial_allocation.loggers,
        );
        writer.push_display("food_per_farmer", value.food_per_farmer);
        writer.push_display("wood_per_logger", value.wood_per_logger);
        writer.push_display(
            "food_consumption_per_worker",
            value.food_consumption_per_worker,
        );
        writer.push_display("wood_consumption_per_tick", value.wood_consumption_per_tick);
        writer.push_display("snapshot_policy", value.snapshot_policy.canonical_string());
        writer.push_display("validation_policy", value.validation_policy.as_str());
        writer.push_display("max_inventory", value.max_inventory);
        Ok(writer.finish())
    }

    fn decode(&self, bytes: &[u8]) -> Result<SettlementSimulationConfig, Self::Error> {
        let mut reader = CanonicalLineReader::new(bytes, "settlement config payload")
            .map_err(|error| SerializationError(error.to_string()))?;
        reader
            .expect_value(
                "payload_kind",
                "settlement_config",
                "settlement config payload",
            )
            .map_err(|error| SerializationError(error.to_string()))?;
        reader
            .expect_value(
                "canonical_encoding",
                CANONICAL_TEXT_ENCODING,
                "settlement config payload",
            )
            .map_err(|error| SerializationError(error.to_string()))?;

        let version = parse_u32(
            reader
                .read_value("config_payload_schema_version", "settlement config payload")
                .map_err(|error| SerializationError(error.to_string()))?,
        )?;
        if version != self.schema_version() {
            return Err(SerializationError(format!(
                "unsupported config payload schema version: {version}",
            )));
        }

        let config = SettlementSimulationConfig {
            population: parse_u32(
                reader
                    .read_value("population", "settlement config payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            initial_food: parse_i64(
                reader
                    .read_value("initial_food", "settlement config payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            initial_wood: parse_i64(
                reader
                    .read_value("initial_wood", "settlement config payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            initial_allocation: WorkerAllocation {
                farmers: parse_u32(
                    reader
                        .read_value("initial_allocation_farmers", "settlement config payload")
                        .map_err(|error| SerializationError(error.to_string()))?,
                )?,
                loggers: parse_u32(
                    reader
                        .read_value("initial_allocation_loggers", "settlement config payload")
                        .map_err(|error| SerializationError(error.to_string()))?,
                )?,
            },
            food_per_farmer: parse_i64(
                reader
                    .read_value("food_per_farmer", "settlement config payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            wood_per_logger: parse_i64(
                reader
                    .read_value("wood_per_logger", "settlement config payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            food_consumption_per_worker: parse_i64(
                reader
                    .read_value("food_consumption_per_worker", "settlement config payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            wood_consumption_per_tick: parse_i64(
                reader
                    .read_value("wood_consumption_per_tick", "settlement config payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            snapshot_policy: parse_snapshot_policy(
                reader
                    .read_value("snapshot_policy", "settlement config payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            validation_policy: parse_validation_policy(
                reader
                    .read_value("validation_policy", "settlement config payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
            max_inventory: parse_i64(
                reader
                    .read_value("max_inventory", "settlement config payload")
                    .map_err(|error| SerializationError(error.to_string()))?,
            )?,
        };

        config
            .validate()
            .map_err(|error| SerializationError(error.to_string()))?;

        reader
            .finish("settlement config payload")
            .map_err(|error| SerializationError(error.to_string()))?;

        Ok(config)
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

fn parse_usize(value: &str) -> Result<usize, SerializationError> {
    value
        .parse()
        .map_err(|error| SerializationError(format!("invalid usize `{value}`: {error}")))
}

fn parse_i64(value: &str) -> Result<i64, SerializationError> {
    value
        .parse()
        .map_err(|error| SerializationError(format!("invalid i64 `{value}`: {error}")))
}

fn parse_snapshot_policy(value: &str) -> Result<SnapshotPolicy, SerializationError> {
    SnapshotPolicy::parse(value)
        .ok_or_else(|| SerializationError(format!("invalid snapshot policy `{value}`")))
}

fn parse_validation_policy(value: &str) -> Result<ValidationPolicy, SerializationError> {
    ValidationPolicy::parse(value)
        .ok_or_else(|| SerializationError(format!("invalid validation policy `{value}`")))
}

fn parse_settlement_status(value: &str) -> Result<SettlementStatus, SerializationError> {
    SettlementStatus::parse(value)
        .ok_or_else(|| SerializationError(format!("invalid settlement status `{value}`")))
}
