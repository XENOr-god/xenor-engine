use std::error::Error;
use std::fmt;

pub type Seed = u64;
pub type Tick = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    UnexpectedInputTick {
        expected: Tick,
        got: Tick,
    },
    SchedulerInvariant {
        detail: String,
    },
    ReplayLifecycle {
        detail: String,
    },
    PhaseFailed {
        tick: Tick,
        group: &'static str,
        phase: &'static str,
        reason: String,
    },
    UnsupportedSchemaVersion {
        artifact: &'static str,
        expected: u32,
        got: u32,
    },
    ConfigDecode {
        detail: String,
    },
    ConfigMismatch {
        detail: String,
    },
    ScenarioDecode {
        detail: String,
    },
    ScenarioMismatch {
        detail: String,
    },
    SummaryMismatch {
        detail: String,
    },
    ReplayDecode {
        detail: String,
    },
    SnapshotDecode {
        detail: String,
    },
    SnapshotSerialization {
        tick: Tick,
        reason: String,
    },
    SnapshotMetadataMismatch {
        detail: String,
    },
    SeedMismatch {
        expected: Seed,
        got: Seed,
    },
    ReplayContinuationMismatch {
        detail: String,
    },
    ResumeTickMismatch {
        expected: Tick,
        got: Tick,
    },
    CorruptedArtifact {
        artifact: &'static str,
        detail: String,
    },
    InvariantViolation {
        tick: Tick,
        checkpoint: &'static str,
        detail: String,
    },
    ReplayMismatch {
        tick: Option<Tick>,
        detail: String,
    },
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedInputTick { expected, got } => {
                write!(f, "unexpected input tick: expected {expected}, got {got}")
            }
            Self::SchedulerInvariant { detail } => {
                write!(f, "scheduler invariant violated: {detail}")
            }
            Self::ReplayLifecycle { detail } => {
                write!(f, "replay lifecycle error: {detail}")
            }
            Self::PhaseFailed {
                tick,
                group,
                phase,
                reason,
            } => write!(
                f,
                "phase `{phase}` in group `{group}` failed at tick {tick}: {reason}"
            ),
            Self::UnsupportedSchemaVersion {
                artifact,
                expected,
                got,
            } => write!(
                f,
                "unsupported {artifact} schema version: expected {expected}, got {got}"
            ),
            Self::ConfigDecode { detail } => write!(f, "config decode failed: {detail}"),
            Self::ConfigMismatch { detail } => write!(f, "config mismatch: {detail}"),
            Self::ScenarioDecode { detail } => write!(f, "scenario decode failed: {detail}"),
            Self::ScenarioMismatch { detail } => write!(f, "scenario mismatch: {detail}"),
            Self::SummaryMismatch { detail } => write!(f, "summary mismatch: {detail}"),
            Self::ReplayDecode { detail } => write!(f, "replay decode failed: {detail}"),
            Self::SnapshotDecode { detail } => write!(f, "snapshot decode failed: {detail}"),
            Self::SnapshotSerialization { tick, reason } => {
                write!(f, "snapshot serialization failed at tick {tick}: {reason}")
            }
            Self::SnapshotMetadataMismatch { detail } => {
                write!(f, "snapshot metadata mismatch: {detail}")
            }
            Self::SeedMismatch { expected, got } => {
                write!(f, "seed mismatch: expected {expected}, got {got}")
            }
            Self::ReplayContinuationMismatch { detail } => {
                write!(f, "replay continuation mismatch: {detail}")
            }
            Self::ResumeTickMismatch { expected, got } => {
                write!(f, "resume tick mismatch: expected {expected}, got {got}")
            }
            Self::CorruptedArtifact { artifact, detail } => {
                write!(f, "corrupted {artifact} artifact: {detail}")
            }
            Self::InvariantViolation {
                tick,
                checkpoint,
                detail,
            } => write!(
                f,
                "state invariant violated at tick {tick} during `{checkpoint}`: {detail}"
            ),
            Self::ReplayMismatch { tick, detail } => match tick {
                Some(tick) => write!(f, "replay mismatch at tick {tick}: {detail}"),
                None => write!(f, "replay mismatch: {detail}"),
            },
        }
    }
}

impl Error for EngineError {}

pub fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

pub fn checksum_words(words: &[u64]) -> u64 {
    let mut checksum = 0x6eed_0e9d_a4d9_4a4f;
    for &word in words {
        checksum = mix64(checksum ^ word);
    }
    checksum
}

pub fn checksum_bytes(bytes: &[u8]) -> u64 {
    let mut checksum = 0x6eed_0e9d_a4d9_4a4f;
    let mut chunk = [0u8; 8];

    for part in bytes.chunks(8) {
        chunk.fill(0);
        chunk[..part.len()].copy_from_slice(part);
        checksum = mix64(checksum ^ u64::from_le_bytes(chunk));
    }

    checksum
}

pub fn hash_str(value: &str) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    value.as_bytes().iter().fold(OFFSET, |hash, byte| {
        hash.wrapping_mul(PRIME) ^ u64::from(*byte)
    })
}

pub fn fork_seed(base: Seed, domain: &str) -> Seed {
    mix64(base ^ hash_str(domain))
}

pub fn tick_seed(seed: Seed, tick: Tick) -> Seed {
    mix64(seed ^ tick)
}
