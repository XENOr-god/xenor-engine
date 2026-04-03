use std::fmt;

use crate::core::{EngineError, Seed, Tick};
use crate::input::{Command, InputFrame};
use crate::scheduler::{PhaseDescriptor, PhaseGroup};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhaseMarker {
    pub ordinal: usize,
    pub group: PhaseGroup,
    pub name: &'static str,
}

impl From<PhaseDescriptor> for PhaseMarker {
    fn from(value: PhaseDescriptor) -> Self {
        Self {
            ordinal: value.ordinal,
            group: value.group,
            name: value.name,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapshotCaptureReason {
    PolicyInterval { interval: Tick },
    Manual,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotRecord<Snapshot>
where
    Snapshot: Clone + Eq + fmt::Debug,
{
    pub tick: Tick,
    pub reason: SnapshotCaptureReason,
    pub payload: Snapshot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayTickRecord<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    pub tick: Tick,
    pub input: InputFrame<C>,
    pub tick_seed: Seed,
    pub phase_markers: Vec<PhaseMarker>,
    pub checksum: u64,
    pub snapshot: Option<SnapshotRecord<Snapshot>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingReplayTickRecord<C>
where
    C: Command,
{
    tick: Tick,
    input: InputFrame<C>,
    tick_seed: Seed,
    phase_markers: Vec<PhaseMarker>,
}

pub trait ReplayLog<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    fn begin_tick(&mut self, frame: &InputFrame<C>, tick_seed: Seed) -> Result<(), EngineError>;
    fn record_phase(&mut self, marker: PhaseMarker) -> Result<(), EngineError>;
    fn complete_tick(
        &mut self,
        checksum: u64,
        snapshot: Option<SnapshotRecord<Snapshot>>,
    ) -> Result<(), EngineError>;
    fn records(&self) -> &[ReplayTickRecord<C, Snapshot>];
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InMemoryReplayLog<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    records: Vec<ReplayTickRecord<C, Snapshot>>,
    pending: Option<PendingReplayTickRecord<C>>,
}

impl<C, Snapshot> Default for InMemoryReplayLog<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    fn default() -> Self {
        Self {
            records: Vec::new(),
            pending: None,
        }
    }
}

impl<C, Snapshot> ReplayLog<C, Snapshot> for InMemoryReplayLog<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    fn begin_tick(&mut self, frame: &InputFrame<C>, tick_seed: Seed) -> Result<(), EngineError> {
        if self.pending.is_some() {
            return Err(EngineError::ReplayLifecycle {
                detail: "attempted to begin a tick while another tick is pending".into(),
            });
        }

        self.pending = Some(PendingReplayTickRecord {
            tick: frame.tick,
            input: frame.clone(),
            tick_seed,
            phase_markers: Vec::new(),
        });

        Ok(())
    }

    fn record_phase(&mut self, marker: PhaseMarker) -> Result<(), EngineError> {
        let pending = self
            .pending
            .as_mut()
            .ok_or_else(|| EngineError::ReplayLifecycle {
                detail: format!(
                    "attempted to record phase `{}` without a pending replay tick",
                    marker.name
                ),
            })?;

        pending.phase_markers.push(marker);
        Ok(())
    }

    fn complete_tick(
        &mut self,
        checksum: u64,
        snapshot: Option<SnapshotRecord<Snapshot>>,
    ) -> Result<(), EngineError> {
        let pending = self
            .pending
            .take()
            .ok_or_else(|| EngineError::ReplayLifecycle {
                detail: "attempted to finalize a replay tick without begin_tick".into(),
            })?;

        self.records.push(ReplayTickRecord {
            tick: pending.tick,
            input: pending.input,
            tick_seed: pending.tick_seed,
            phase_markers: pending.phase_markers,
            checksum,
            snapshot,
        });

        Ok(())
    }

    fn records(&self) -> &[ReplayTickRecord<C, Snapshot>] {
        &self.records
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayMismatchKind<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    MissingActualTick,
    UnexpectedActualTick,
    Tick {
        expected: Tick,
        actual: Tick,
    },
    Input {
        expected: InputFrame<C>,
        actual: InputFrame<C>,
    },
    TickSeed {
        expected: Seed,
        actual: Seed,
    },
    PhaseMarkers {
        expected: Vec<PhaseMarker>,
        actual: Vec<PhaseMarker>,
    },
    Checksum {
        expected: u64,
        actual: u64,
    },
    SnapshotPresence {
        expected: bool,
        actual: bool,
    },
    SnapshotPayload {
        expected: SnapshotRecord<Snapshot>,
        actual: SnapshotRecord<Snapshot>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayDivergence<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    pub record_index: usize,
    pub tick: Option<Tick>,
    pub kind: ReplayMismatchKind<C, Snapshot>,
}

impl<C, Snapshot> fmt::Display for ReplayDivergence<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ReplayMismatchKind::MissingActualTick => write!(
                f,
                "missing actual replay record at index {} for tick {:?}",
                self.record_index, self.tick
            ),
            ReplayMismatchKind::UnexpectedActualTick => write!(
                f,
                "unexpected actual replay record at index {} for tick {:?}",
                self.record_index, self.tick
            ),
            ReplayMismatchKind::Tick { expected, actual } => {
                write!(f, "tick mismatch: expected {expected}, got {actual}")
            }
            ReplayMismatchKind::Input { expected, actual } => {
                write!(
                    f,
                    "input mismatch: expected {:?}, got {:?}",
                    expected, actual
                )
            }
            ReplayMismatchKind::TickSeed { expected, actual } => {
                write!(f, "tick seed mismatch: expected {expected}, got {actual}")
            }
            ReplayMismatchKind::PhaseMarkers { expected, actual } => {
                write!(
                    f,
                    "phase marker mismatch: expected {:?}, got {:?}",
                    expected, actual
                )
            }
            ReplayMismatchKind::Checksum { expected, actual } => {
                write!(f, "checksum mismatch: expected {expected}, got {actual}")
            }
            ReplayMismatchKind::SnapshotPresence { expected, actual } => write!(
                f,
                "snapshot presence mismatch: expected {expected}, got {actual}"
            ),
            ReplayMismatchKind::SnapshotPayload { expected, actual } => write!(
                f,
                "snapshot payload mismatch: expected {:?}, got {:?}",
                expected, actual
            ),
        }
    }
}

impl<C, Snapshot> From<ReplayDivergence<C, Snapshot>> for EngineError
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    fn from(value: ReplayDivergence<C, Snapshot>) -> Self {
        EngineError::ReplayMismatch {
            tick: value.tick,
            detail: value.to_string(),
        }
    }
}

pub fn compare_replay_traces<C, Snapshot>(
    expected: &[ReplayTickRecord<C, Snapshot>],
    actual: &[ReplayTickRecord<C, Snapshot>],
) -> Result<(), ReplayDivergence<C, Snapshot>>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    let shared_len = expected.len().min(actual.len());

    for index in 0..shared_len {
        let expected_record = &expected[index];
        let actual_record = &actual[index];
        let tick = Some(expected_record.tick);

        if expected_record.tick != actual_record.tick {
            return Err(ReplayDivergence {
                record_index: index,
                tick,
                kind: ReplayMismatchKind::Tick {
                    expected: expected_record.tick,
                    actual: actual_record.tick,
                },
            });
        }

        if expected_record.input != actual_record.input {
            return Err(ReplayDivergence {
                record_index: index,
                tick,
                kind: ReplayMismatchKind::Input {
                    expected: expected_record.input.clone(),
                    actual: actual_record.input.clone(),
                },
            });
        }

        if expected_record.tick_seed != actual_record.tick_seed {
            return Err(ReplayDivergence {
                record_index: index,
                tick,
                kind: ReplayMismatchKind::TickSeed {
                    expected: expected_record.tick_seed,
                    actual: actual_record.tick_seed,
                },
            });
        }

        if expected_record.phase_markers != actual_record.phase_markers {
            return Err(ReplayDivergence {
                record_index: index,
                tick,
                kind: ReplayMismatchKind::PhaseMarkers {
                    expected: expected_record.phase_markers.clone(),
                    actual: actual_record.phase_markers.clone(),
                },
            });
        }

        if expected_record.checksum != actual_record.checksum {
            return Err(ReplayDivergence {
                record_index: index,
                tick,
                kind: ReplayMismatchKind::Checksum {
                    expected: expected_record.checksum,
                    actual: actual_record.checksum,
                },
            });
        }

        if expected_record.snapshot.is_some() != actual_record.snapshot.is_some() {
            return Err(ReplayDivergence {
                record_index: index,
                tick,
                kind: ReplayMismatchKind::SnapshotPresence {
                    expected: expected_record.snapshot.is_some(),
                    actual: actual_record.snapshot.is_some(),
                },
            });
        }

        if expected_record.snapshot != actual_record.snapshot {
            return Err(ReplayDivergence {
                record_index: index,
                tick,
                kind: ReplayMismatchKind::SnapshotPayload {
                    expected: expected_record
                        .snapshot
                        .clone()
                        .expect("snapshot presence already checked"),
                    actual: actual_record
                        .snapshot
                        .clone()
                        .expect("snapshot presence already checked"),
                },
            });
        }
    }

    if expected.len() > actual.len() {
        let record = &expected[shared_len];
        return Err(ReplayDivergence {
            record_index: shared_len,
            tick: Some(record.tick),
            kind: ReplayMismatchKind::MissingActualTick,
        });
    }

    if actual.len() > expected.len() {
        let record = &actual[shared_len];
        return Err(ReplayDivergence {
            record_index: shared_len,
            tick: Some(record.tick),
            kind: ReplayMismatchKind::UnexpectedActualTick,
        });
    }

    Ok(())
}
