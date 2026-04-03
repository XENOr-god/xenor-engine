use std::fmt;

use crate::core::{EngineError, Seed, Tick};
use crate::persistence::ArtifactSummary;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParityArtifactSummary {
    pub engine_family: String,
    pub base_seed: Seed,
    pub final_tick: Tick,
    pub final_checksum: u64,
    pub config_payload_schema_version: u32,
    pub config_digest: u64,
    pub replay_artifact_schema_version: u32,
    pub snapshot_artifact_schema_version: u32,
    pub command_payload_schema_version: u32,
    pub snapshot_payload_schema_version: u32,
    pub replay_digest: u64,
    pub snapshot_digest: Option<u64>,
    pub scenario_digest: Option<u64>,
}

impl From<&ArtifactSummary> for ParityArtifactSummary {
    fn from(value: &ArtifactSummary) -> Self {
        Self {
            engine_family: value.engine_family.clone(),
            base_seed: value.base_seed,
            final_tick: value.final_tick,
            final_checksum: value.final_checksum,
            config_payload_schema_version: value.config_payload_schema_version,
            config_digest: value.config_digest,
            replay_artifact_schema_version: value.replay_artifact_schema_version,
            snapshot_artifact_schema_version: value.snapshot_artifact_schema_version,
            command_payload_schema_version: value.command_payload_schema_version,
            snapshot_payload_schema_version: value.snapshot_payload_schema_version,
            replay_digest: value.replay_digest,
            snapshot_digest: value.snapshot_digest,
            scenario_digest: value.scenario_digest,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParityMismatch {
    BaseSeed {
        expected: Seed,
        actual: Seed,
    },
    FinalTick {
        expected: Tick,
        actual: Tick,
    },
    FinalChecksum {
        expected: u64,
        actual: u64,
    },
    ConfigSchemaVersion {
        expected: u32,
        actual: u32,
    },
    ConfigDigest {
        expected: u64,
        actual: u64,
    },
    ReplayDigest {
        expected: u64,
        actual: u64,
    },
    SnapshotDigest {
        expected: Option<u64>,
        actual: Option<u64>,
    },
    ScenarioDigest {
        expected: Option<u64>,
        actual: Option<u64>,
    },
}

impl fmt::Display for ParityMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BaseSeed { expected, actual } => {
                write!(f, "base seed mismatch: expected {expected}, got {actual}")
            }
            Self::FinalTick { expected, actual } => {
                write!(f, "final tick mismatch: expected {expected}, got {actual}")
            }
            Self::FinalChecksum { expected, actual } => {
                write!(
                    f,
                    "final checksum mismatch: expected {expected}, got {actual}"
                )
            }
            Self::ConfigSchemaVersion { expected, actual } => {
                write!(
                    f,
                    "config schema version mismatch: expected {expected}, got {actual}"
                )
            }
            Self::ConfigDigest { expected, actual } => {
                write!(
                    f,
                    "config digest mismatch: expected {expected}, got {actual}"
                )
            }
            Self::ReplayDigest { expected, actual } => {
                write!(
                    f,
                    "replay digest mismatch: expected {expected}, got {actual}"
                )
            }
            Self::SnapshotDigest { expected, actual } => {
                write!(
                    f,
                    "snapshot digest mismatch: expected {:?}, got {:?}",
                    expected, actual
                )
            }
            Self::ScenarioDigest { expected, actual } => {
                write!(
                    f,
                    "scenario digest mismatch: expected {:?}, got {:?}",
                    expected, actual
                )
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParityComparison {
    pub expected: ParityArtifactSummary,
    pub actual: ParityArtifactSummary,
    pub mismatches: Vec<ParityMismatch>,
}

impl ParityComparison {
    pub fn is_match(&self) -> bool {
        self.mismatches.is_empty()
    }

    pub fn first_mismatch(&self) -> Option<&ParityMismatch> {
        self.mismatches.first()
    }

    pub fn into_result(self) -> Result<Self, EngineError> {
        if let Some(mismatch) = self.first_mismatch() {
            return Err(EngineError::ReplayMismatch {
                tick: Some(self.actual.final_tick),
                detail: mismatch.to_string(),
            });
        }

        Ok(self)
    }
}

impl fmt::Display for ParityComparison {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.mismatches.is_empty() {
            return write!(
                f,
                "parity match at final tick {} with replay digest {}",
                self.actual.final_tick, self.actual.replay_digest
            );
        }

        let details = self
            .mismatches
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        write!(f, "parity mismatch: {details}")
    }
}

pub fn compare_parity_summaries(
    expected: &ParityArtifactSummary,
    actual: &ParityArtifactSummary,
) -> ParityComparison {
    let mut mismatches = Vec::new();

    if expected.base_seed != actual.base_seed {
        mismatches.push(ParityMismatch::BaseSeed {
            expected: expected.base_seed,
            actual: actual.base_seed,
        });
    }

    if expected.final_tick != actual.final_tick {
        mismatches.push(ParityMismatch::FinalTick {
            expected: expected.final_tick,
            actual: actual.final_tick,
        });
    }

    if expected.final_checksum != actual.final_checksum {
        mismatches.push(ParityMismatch::FinalChecksum {
            expected: expected.final_checksum,
            actual: actual.final_checksum,
        });
    }

    if expected.config_payload_schema_version != actual.config_payload_schema_version {
        mismatches.push(ParityMismatch::ConfigSchemaVersion {
            expected: expected.config_payload_schema_version,
            actual: actual.config_payload_schema_version,
        });
    }

    if expected.config_digest != actual.config_digest {
        mismatches.push(ParityMismatch::ConfigDigest {
            expected: expected.config_digest,
            actual: actual.config_digest,
        });
    }

    if expected.replay_digest != actual.replay_digest {
        mismatches.push(ParityMismatch::ReplayDigest {
            expected: expected.replay_digest,
            actual: actual.replay_digest,
        });
    }

    if expected.snapshot_digest != actual.snapshot_digest {
        mismatches.push(ParityMismatch::SnapshotDigest {
            expected: expected.snapshot_digest,
            actual: actual.snapshot_digest,
        });
    }

    if expected.scenario_digest != actual.scenario_digest {
        mismatches.push(ParityMismatch::ScenarioDigest {
            expected: expected.scenario_digest,
            actual: actual.scenario_digest,
        });
    }

    ParityComparison {
        expected: expected.clone(),
        actual: actual.clone(),
        mismatches,
    }
}
