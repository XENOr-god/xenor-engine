use crate::core::{Seed, Tick, checksum_words, mix64, tick_seed};

pub trait SimulationState: Clone {
    type Snapshot: Clone + Eq + std::fmt::Debug;

    fn tick(&self) -> Tick;
    fn set_tick(&mut self, tick: Tick);
    fn checksum(&self) -> u64;
    fn snapshot(&self) -> Self::Snapshot;
    fn restore_snapshot(&mut self, snapshot: Self::Snapshot);
    fn snapshot_schema_version() -> u32
    where
        Self: Sized;
    fn snapshot_checksum(snapshot: &Self::Snapshot) -> u64
    where
        Self: Sized;
    fn snapshot_tick(snapshot: &Self::Snapshot) -> Tick
    where
        Self: Sized;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CounterState {
    tick: Tick,
    value: i64,
    velocity: i64,
    pending_delta: i64,
    pending_entropy: u64,
    entropy_budget: u64,
    finalize_marker: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CounterSnapshot {
    pub tick: Tick,
    pub value: i64,
    pub velocity: i64,
    pub pending_delta: i64,
    pub pending_entropy: u64,
    pub entropy_budget: u64,
    pub finalize_marker: u64,
}

impl CounterState {
    pub fn with_initial_conditions(value: i64, velocity: i64) -> Self {
        Self {
            value,
            velocity,
            ..Self::default()
        }
    }

    pub const fn value(&self) -> i64 {
        self.value
    }

    pub const fn velocity(&self) -> i64 {
        self.velocity
    }

    pub const fn pending_delta(&self) -> i64 {
        self.pending_delta
    }

    pub const fn pending_entropy(&self) -> u64 {
        self.pending_entropy
    }

    pub const fn entropy_budget(&self) -> u64 {
        self.entropy_budget
    }

    pub const fn finalize_marker(&self) -> u64 {
        self.finalize_marker
    }

    pub fn reset_finalize_marker(&mut self) {
        self.finalize_marker = 0;
    }

    pub fn stage_input(&mut self, delta: i64, entropy: u64) {
        self.pending_delta = self.pending_delta.saturating_add(delta);
        self.pending_entropy = self.pending_entropy.wrapping_add(entropy);
    }

    pub fn simulate(&mut self) {
        self.velocity = self.velocity.saturating_add(self.pending_delta);

        let entropy_impulse =
            i64::try_from(self.pending_entropy & 0xff).expect("masked entropy always fits in i64");
        self.value = self
            .value
            .saturating_add(self.velocity)
            .saturating_add(entropy_impulse);
    }

    pub fn settle(&mut self) {
        self.entropy_budget = self.entropy_budget.wrapping_add(self.pending_entropy);
        self.pending_delta = 0;
        self.pending_entropy = 0;
    }

    pub fn finalize(&mut self, marker: u64) {
        self.finalize_marker = marker;
    }

    pub(crate) fn snapshot_checksum_words(snapshot: &CounterSnapshot) -> [u64; 7] {
        [
            snapshot.tick,
            snapshot.value as u64,
            snapshot.velocity as u64,
            snapshot.pending_delta as u64,
            snapshot.pending_entropy,
            snapshot.entropy_budget,
            snapshot.finalize_marker,
        ]
    }

    pub fn preview_finalize_marker(&self, seed: Seed, tick: Tick) -> u64 {
        let mut snapshot = self.snapshot();
        snapshot.finalize_marker = 0;
        mix64(
            tick_seed(seed, tick)
                ^ checksum_words(&Self::snapshot_checksum_words(&snapshot))
                ^ tick,
        )
    }
}

impl SimulationState for CounterState {
    type Snapshot = CounterSnapshot;

    fn tick(&self) -> Tick {
        self.tick
    }

    fn set_tick(&mut self, tick: Tick) {
        self.tick = tick;
    }

    fn checksum(&self) -> u64 {
        checksum_words(&Self::snapshot_checksum_words(&self.snapshot()))
    }

    fn snapshot(&self) -> Self::Snapshot {
        CounterSnapshot {
            tick: self.tick,
            value: self.value,
            velocity: self.velocity,
            pending_delta: self.pending_delta,
            pending_entropy: self.pending_entropy,
            entropy_budget: self.entropy_budget,
            finalize_marker: self.finalize_marker,
        }
    }

    fn restore_snapshot(&mut self, snapshot: Self::Snapshot) {
        self.tick = snapshot.tick;
        self.value = snapshot.value;
        self.velocity = snapshot.velocity;
        self.pending_delta = snapshot.pending_delta;
        self.pending_entropy = snapshot.pending_entropy;
        self.entropy_budget = snapshot.entropy_budget;
        self.finalize_marker = snapshot.finalize_marker;
    }

    fn snapshot_schema_version() -> u32
    where
        Self: Sized,
    {
        1
    }

    fn snapshot_checksum(snapshot: &Self::Snapshot) -> u64
    where
        Self: Sized,
    {
        checksum_words(&Self::snapshot_checksum_words(snapshot))
    }

    fn snapshot_tick(snapshot: &Self::Snapshot) -> Tick
    where
        Self: Sized,
    {
        snapshot.tick
    }
}
