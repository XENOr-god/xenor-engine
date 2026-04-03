use std::fmt;
use std::marker::PhantomData;

use crate::canonical::{
    CANONICAL_TEXT_ENCODING, CanonicalError, CanonicalLineReader, CanonicalLineWriter,
    canonical_digest, decode_hex, decode_hex_string, encode_hex,
};
use crate::core::EngineError;
use crate::deterministic::DeterministicList;
use crate::engine::SnapshotPolicy;
use crate::serialization::Serializer;
use crate::state::CounterEntityInit;
use crate::validation::ValidationPolicy;

pub const CONFIG_ARTIFACT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConfigIdentity {
    pub payload_schema_version: u32,
    pub digest: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigArtifactMetadata {
    pub artifact_schema_version: u32,
    pub engine_family: String,
    pub identity: ConfigIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigArtifact<Config>
where
    Config: Clone + Eq + fmt::Debug,
{
    pub metadata: ConfigArtifactMetadata,
    pub payload: Config,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CounterSimulationConfig {
    pub initial_value: i64,
    pub initial_velocity: i64,
    pub initial_entities: DeterministicList<CounterEntityInit>,
    pub snapshot_policy: SnapshotPolicy,
    pub validation_policy: ValidationPolicy,
    pub max_abs_value: i64,
    pub max_abs_velocity: i64,
    pub max_abs_pending_delta: i64,
}

pub trait SimulationConfig: Clone + Eq + fmt::Debug {
    fn snapshot_policy(&self) -> SnapshotPolicy;
}

impl CounterSimulationConfig {
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.max_abs_value < 0 {
            return Err(EngineError::ConfigMismatch {
                detail: format!(
                    "counter config max_abs_value must be non-negative, got {}",
                    self.max_abs_value
                ),
            });
        }

        if self.max_abs_velocity < 0 {
            return Err(EngineError::ConfigMismatch {
                detail: format!(
                    "counter config max_abs_velocity must be non-negative, got {}",
                    self.max_abs_velocity
                ),
            });
        }

        if self.max_abs_pending_delta < 0 {
            return Err(EngineError::ConfigMismatch {
                detail: format!(
                    "counter config max_abs_pending_delta must be non-negative, got {}",
                    self.max_abs_pending_delta
                ),
            });
        }

        self.validate_entity_value("initial_value", self.initial_value)?;
        self.validate_entity_value("initial_velocity", self.initial_velocity)?;

        for (index, entity) in self.initial_entities.iter().enumerate() {
            self.validate_entity_value(
                format!("initial_entities[{index}].value").as_str(),
                entity.value,
            )?;
            self.validate_entity_value(
                format!("initial_entities[{index}].velocity").as_str(),
                entity.velocity,
            )?;
        }

        Ok(())
    }

    fn validate_entity_value(&self, label: &str, value: i64) -> Result<(), EngineError> {
        let limit = if label.ends_with("velocity") {
            self.max_abs_velocity
        } else {
            self.max_abs_value
        };
        let magnitude = i128::from(value).abs();
        if magnitude > i128::from(limit) {
            return Err(EngineError::ConfigMismatch {
                detail: format!(
                    "{label} exceeds deterministic limit: value={value}, limit={limit}"
                ),
            });
        }

        Ok(())
    }
}

impl SimulationConfig for CounterSimulationConfig {
    fn snapshot_policy(&self) -> SnapshotPolicy {
        self.snapshot_policy
    }
}

impl Default for CounterSimulationConfig {
    fn default() -> Self {
        Self {
            initial_value: 0,
            initial_velocity: 0,
            initial_entities: DeterministicList::new(),
            snapshot_policy: SnapshotPolicy::Every { interval: 1 },
            validation_policy: ValidationPolicy::TickBoundary,
            max_abs_value: i64::MAX,
            max_abs_velocity: i64::MAX,
            max_abs_pending_delta: i64::MAX,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ConfigArtifactSerializer<Config, PayloadSerializer> {
    engine_family: String,
    payload_serializer: PayloadSerializer,
    _marker: PhantomData<Config>,
}

impl<Config, PayloadSerializer> ConfigArtifactSerializer<Config, PayloadSerializer> {
    pub fn new(engine_family: impl Into<String>, payload_serializer: PayloadSerializer) -> Self {
        Self {
            engine_family: engine_family.into(),
            payload_serializer,
            _marker: PhantomData,
        }
    }
}

impl<Config, PayloadSerializer> ConfigArtifactSerializer<Config, PayloadSerializer>
where
    Config: Clone + Eq + fmt::Debug,
    PayloadSerializer: Serializer<Config>,
    PayloadSerializer::Error: fmt::Display,
{
    pub fn build_artifact(&self, payload: &Config) -> Result<ConfigArtifact<Config>, EngineError> {
        let identity = self.identity(payload)?;
        Ok(ConfigArtifact {
            metadata: ConfigArtifactMetadata {
                artifact_schema_version: CONFIG_ARTIFACT_SCHEMA_VERSION,
                engine_family: self.engine_family.clone(),
                identity,
            },
            payload: payload.clone(),
        })
    }

    pub fn identity(&self, payload: &Config) -> Result<ConfigIdentity, EngineError> {
        let payload_bytes =
            self.payload_serializer
                .encode(payload)
                .map_err(|error| EngineError::ConfigDecode {
                    detail: format!("failed to encode config payload: {error}"),
                })?;
        Ok(ConfigIdentity {
            payload_schema_version: self.payload_serializer.schema_version(),
            digest: canonical_digest(&payload_bytes),
        })
    }

    pub fn digest(&self, artifact: &ConfigArtifact<Config>) -> Result<u64, EngineError> {
        self.encode(artifact).map(|bytes| canonical_digest(&bytes))
    }
}

impl<Config, PayloadSerializer> Serializer<ConfigArtifact<Config>>
    for ConfigArtifactSerializer<Config, PayloadSerializer>
where
    Config: Clone + Eq + fmt::Debug,
    PayloadSerializer: Serializer<Config>,
    PayloadSerializer::Error: fmt::Display,
{
    type Error = EngineError;

    fn schema_version(&self) -> u32 {
        CONFIG_ARTIFACT_SCHEMA_VERSION
    }

    fn encode(&self, value: &ConfigArtifact<Config>) -> Result<Vec<u8>, Self::Error> {
        if value.metadata.artifact_schema_version != self.schema_version() {
            return Err(EngineError::UnsupportedSchemaVersion {
                artifact: "config artifact",
                expected: self.schema_version(),
                got: value.metadata.artifact_schema_version,
            });
        }

        if value.metadata.engine_family != self.engine_family {
            return Err(EngineError::ConfigMismatch {
                detail: format!(
                    "config engine family mismatch: expected `{}`, got `{}`",
                    self.engine_family, value.metadata.engine_family
                ),
            });
        }

        if value.metadata.identity.payload_schema_version
            != self.payload_serializer.schema_version()
        {
            return Err(EngineError::UnsupportedSchemaVersion {
                artifact: "config payload",
                expected: self.payload_serializer.schema_version(),
                got: value.metadata.identity.payload_schema_version,
            });
        }

        let payload_bytes = self
            .payload_serializer
            .encode(&value.payload)
            .map_err(|error| EngineError::ConfigDecode {
                detail: format!("failed to encode config payload: {error}"),
            })?;
        let payload_digest = canonical_digest(&payload_bytes);
        if payload_digest != value.metadata.identity.digest {
            return Err(EngineError::ConfigMismatch {
                detail: format!(
                    "config digest mismatch: metadata={}, payload={payload_digest}",
                    value.metadata.identity.digest
                ),
            });
        }

        let mut writer = CanonicalLineWriter::default();
        writer.push_display("artifact", "config");
        writer.push_display("canonical_encoding", CANONICAL_TEXT_ENCODING);
        writer.push_display(
            "artifact_schema_version",
            value.metadata.artifact_schema_version,
        );
        writer.push_str_hex("engine_family_hex", &value.metadata.engine_family);
        writer.push_display(
            "config_payload_schema_version",
            value.metadata.identity.payload_schema_version,
        );
        writer.push_display("config_digest", value.metadata.identity.digest);
        writer.push_display("payload_hex", encode_hex(&payload_bytes));
        Ok(writer.finish())
    }

    fn decode(&self, bytes: &[u8]) -> Result<ConfigArtifact<Config>, Self::Error> {
        let mut reader = CanonicalLineReader::new(bytes, "config artifact")
            .map_err(config_corrupted_canonical)?;
        expect_config_value(&mut reader, "artifact", "config")?;
        expect_config_value(&mut reader, "canonical_encoding", CANONICAL_TEXT_ENCODING)?;

        let artifact_schema_version =
            parse_u32(read_config_value(&mut reader, "artifact_schema_version")?)?;
        if artifact_schema_version != self.schema_version() {
            return Err(EngineError::UnsupportedSchemaVersion {
                artifact: "config artifact",
                expected: self.schema_version(),
                got: artifact_schema_version,
            });
        }

        let engine_family = decode_hex_string(
            read_config_value(&mut reader, "engine_family_hex")?,
            "config artifact engine family",
        )
        .map_err(config_corrupted_canonical)?;
        if engine_family != self.engine_family {
            return Err(EngineError::ConfigDecode {
                detail: format!(
                    "engine family mismatch: expected `{}`, got `{engine_family}`",
                    self.engine_family
                ),
            });
        }

        let payload_schema_version = parse_u32(read_config_value(
            &mut reader,
            "config_payload_schema_version",
        )?)?;
        if payload_schema_version != self.payload_serializer.schema_version() {
            return Err(EngineError::UnsupportedSchemaVersion {
                artifact: "config payload",
                expected: self.payload_serializer.schema_version(),
                got: payload_schema_version,
            });
        }

        let digest = parse_u64(read_config_value(&mut reader, "config_digest")?)?;
        let payload_hex = read_config_value(&mut reader, "payload_hex")?;
        let payload_bytes = decode_hex(payload_hex, "config artifact payload")
            .map_err(config_corrupted_canonical)?;
        let payload = self
            .payload_serializer
            .decode(&payload_bytes)
            .map_err(|error| EngineError::ConfigDecode {
                detail: error.to_string(),
            })?;
        reader
            .finish("config artifact")
            .map_err(config_corrupted_canonical)?;

        let computed_identity = self.identity(&payload)?;
        if computed_identity.digest != digest {
            return Err(EngineError::ConfigMismatch {
                detail: format!(
                    "config digest mismatch: metadata={digest}, payload={}",
                    computed_identity.digest
                ),
            });
        }

        Ok(ConfigArtifact {
            metadata: ConfigArtifactMetadata {
                artifact_schema_version,
                engine_family,
                identity: computed_identity,
            },
            payload,
        })
    }
}

fn config_corrupted_canonical(error: CanonicalError) -> EngineError {
    EngineError::CorruptedArtifact {
        artifact: "config",
        detail: error.to_string(),
    }
}

fn read_config_value<'a>(
    reader: &mut CanonicalLineReader<'a>,
    key: &str,
) -> Result<&'a str, EngineError> {
    reader
        .read_value(key, "config artifact")
        .map_err(config_corrupted_canonical)
}

fn expect_config_value(
    reader: &mut CanonicalLineReader<'_>,
    key: &str,
    expected: &str,
) -> Result<(), EngineError> {
    reader
        .expect_value(key, expected, "config artifact")
        .map_err(config_corrupted_canonical)
}

fn parse_u32(value: &str) -> Result<u32, EngineError> {
    value
        .parse()
        .map_err(|error| EngineError::CorruptedArtifact {
            artifact: "config",
            detail: format!("invalid u32 `{value}`: {error}"),
        })
}

fn parse_u64(value: &str) -> Result<u64, EngineError> {
    value
        .parse()
        .map_err(|error| EngineError::CorruptedArtifact {
            artifact: "config",
            detail: format!("invalid u64 `{value}`: {error}"),
        })
}
