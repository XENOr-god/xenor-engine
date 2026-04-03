use crate::core::{Seed, Tick, checksum_words, mix64, tick_seed};
use crate::deterministic::{DeterministicList, DeterministicMap};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CounterEntityInit {
    pub value: i64,
    pub velocity: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CounterEntitySnapshot {
    pub id: EntityId,
    pub value: i64,
    pub velocity: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CounterEntity {
    value: i64,
    velocity: i64,
}

impl CounterEntity {
    fn from_init(init: CounterEntityInit) -> Self {
        Self {
            value: init.value,
            velocity: init.velocity,
        }
    }

    fn snapshot(&self, id: EntityId) -> CounterEntitySnapshot {
        CounterEntitySnapshot {
            id,
            value: self.value,
            velocity: self.velocity,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct CounterTransientState {
    pending_delta: i64,
    pending_entropy: u64,
}

/// Authoritative counter simulation state.
///
/// Runtime callers can inspect the state, but domain mutation methods remain
/// crate-private so authoritative changes only happen through phase execution.
///
/// ```compile_fail
/// use xenor_engine_rust::CounterState;
///
/// let mut state = CounterState::default();
/// state.stage_input(3, 0);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CounterState {
    tick: Tick,
    next_entity_id: u64,
    primary_entity: EntityId,
    entity_order: DeterministicList<EntityId>,
    entities: DeterministicMap<EntityId, CounterEntity>,
    transient: CounterTransientState,
    entropy_budget: u64,
    finalize_marker: u64,
}

impl Default for CounterState {
    fn default() -> Self {
        Self::with_initial_entities(0, 0, &DeterministicList::new())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CounterSnapshot {
    pub tick: Tick,
    pub next_entity_id: u64,
    pub primary_entity: EntityId,
    pub entities: DeterministicList<CounterEntitySnapshot>,
    pub pending_delta: i64,
    pub pending_entropy: u64,
    pub entropy_budget: u64,
    pub finalize_marker: u64,
}

impl CounterSnapshot {
    pub fn validate(&self) -> Result<(), String> {
        if self.entities.is_empty() {
            return Err("counter snapshot must contain at least one entity".into());
        }

        if self.entities.get(0).map(|entity| entity.id) != Some(self.primary_entity) {
            return Err(format!(
                "counter snapshot primary entity {:?} must be first in insertion order",
                self.primary_entity
            ));
        }

        let mut seen = DeterministicMap::new();
        let mut max_entity_id = 0;
        for entity in &self.entities {
            if seen.insert(entity.id, ()).is_some() {
                return Err(format!(
                    "duplicate entity id {:?} in counter snapshot",
                    entity.id
                ));
            }
            max_entity_id = max_entity_id.max(entity.id.0);
        }

        if !seen.contains_key(&self.primary_entity) {
            return Err(format!(
                "counter snapshot primary entity {:?} missing from entity payload",
                self.primary_entity
            ));
        }

        if self.next_entity_id <= max_entity_id {
            return Err(format!(
                "counter snapshot next_entity_id {} must be greater than max entity id {}",
                self.next_entity_id, max_entity_id
            ));
        }

        Ok(())
    }
}

impl CounterState {
    pub fn with_initial_conditions(value: i64, velocity: i64) -> Self {
        Self::with_initial_entities(value, velocity, &DeterministicList::new())
    }

    pub fn with_initial_entities(
        value: i64,
        velocity: i64,
        extra_entities: &DeterministicList<CounterEntityInit>,
    ) -> Self {
        let primary_entity = EntityId(1);
        let mut entity_order = DeterministicList::new();
        let mut entities = DeterministicMap::new();
        entity_order.push(primary_entity);
        entities.insert(primary_entity, CounterEntity { value, velocity });

        let mut state = Self {
            tick: 0,
            next_entity_id: 2,
            primary_entity,
            entity_order,
            entities,
            transient: CounterTransientState::default(),
            entropy_budget: 0,
            finalize_marker: 0,
        };

        for extra in extra_entities {
            state.spawn_entity(extra.clone());
        }

        state
    }

    pub fn value(&self) -> i64 {
        self.primary_entity_ref().value
    }

    pub fn velocity(&self) -> i64 {
        self.primary_entity_ref().velocity
    }

    pub const fn pending_delta(&self) -> i64 {
        self.transient.pending_delta
    }

    pub const fn pending_entropy(&self) -> u64 {
        self.transient.pending_entropy
    }

    pub const fn entropy_budget(&self) -> u64 {
        self.entropy_budget
    }

    pub const fn finalize_marker(&self) -> u64 {
        self.finalize_marker
    }

    pub const fn primary_entity_id(&self) -> EntityId {
        self.primary_entity
    }

    pub fn next_entity_id(&self) -> EntityId {
        EntityId(self.next_entity_id)
    }

    pub fn entity_count(&self) -> usize {
        self.entity_order.len()
    }

    pub fn entity_ids(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.entity_order.iter().copied()
    }

    pub fn entity_snapshot(&self, id: EntityId) -> Option<CounterEntitySnapshot> {
        self.entities.get(&id).map(|entity| entity.snapshot(id))
    }

    pub fn entity_snapshots(&self) -> DeterministicList<CounterEntitySnapshot> {
        self.entity_order
            .iter()
            .filter_map(|id| self.entity_snapshot(*id))
            .collect()
    }

    pub(crate) fn reset_finalize_marker(&mut self) {
        self.finalize_marker = 0;
    }

    pub(crate) fn stage_input(&mut self, delta: i64, entropy: u64) {
        self.transient.pending_delta = self.transient.pending_delta.saturating_add(delta);
        self.transient.pending_entropy = self.transient.pending_entropy.wrapping_add(entropy);
    }

    pub(crate) fn simulate(&mut self) {
        let pending_delta = self.transient.pending_delta;
        let pending_entropy = self.transient.pending_entropy;
        let primary_entity = self.primary_entity_mut();

        primary_entity.velocity = primary_entity.velocity.saturating_add(pending_delta);

        let entropy_impulse =
            i64::try_from(pending_entropy & 0xff).expect("masked entropy always fits in i64");
        primary_entity.value = primary_entity
            .value
            .saturating_add(primary_entity.velocity)
            .saturating_add(entropy_impulse);
    }

    pub(crate) fn settle(&mut self) {
        self.entropy_budget = self
            .entropy_budget
            .wrapping_add(self.transient.pending_entropy);
        self.transient.pending_delta = 0;
        self.transient.pending_entropy = 0;
    }

    pub(crate) fn finalize(&mut self, marker: u64) {
        self.finalize_marker = marker;
    }

    pub(crate) fn spawn_entity(&mut self, init: CounterEntityInit) -> EntityId {
        let entity_id = EntityId(self.next_entity_id);
        self.next_entity_id = self.next_entity_id.wrapping_add(1);
        self.entity_order.push(entity_id);
        self.entities
            .insert(entity_id, CounterEntity::from_init(init));
        entity_id
    }

    pub(crate) fn snapshot_checksum_words(snapshot: &CounterSnapshot) -> Vec<u64> {
        let mut words = Vec::with_capacity(8 + snapshot.entities.len() * 3);
        words.push(snapshot.tick);
        words.push(snapshot.next_entity_id);
        words.push(snapshot.primary_entity.0);
        words.push(snapshot.entities.len() as u64);
        for entity in &snapshot.entities {
            words.push(entity.id.0);
            words.push(entity.value as u64);
            words.push(entity.velocity as u64);
        }
        words.push(snapshot.pending_delta as u64);
        words.push(snapshot.pending_entropy);
        words.push(snapshot.entropy_budget);
        words.push(snapshot.finalize_marker);
        words
    }

    pub fn preview_finalize_marker(&self, seed: Seed, tick: Tick) -> u64 {
        let mut snapshot = self.snapshot();
        snapshot.finalize_marker = 0;
        let words = Self::snapshot_checksum_words(&snapshot);
        mix64(tick_seed(seed, tick) ^ checksum_words(&words) ^ tick)
    }

    fn primary_entity_ref(&self) -> &CounterEntity {
        self.entities
            .get(&self.primary_entity)
            .expect("counter state primary entity must exist")
    }

    fn primary_entity_mut(&mut self) -> &mut CounterEntity {
        self.entities
            .get_mut(&self.primary_entity)
            .expect("counter state primary entity must exist")
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
        let snapshot = self.snapshot();
        let words = Self::snapshot_checksum_words(&snapshot);
        checksum_words(&words)
    }

    fn snapshot(&self) -> Self::Snapshot {
        CounterSnapshot {
            tick: self.tick,
            next_entity_id: self.next_entity_id,
            primary_entity: self.primary_entity,
            entities: self.entity_snapshots(),
            pending_delta: self.transient.pending_delta,
            pending_entropy: self.transient.pending_entropy,
            entropy_budget: self.entropy_budget,
            finalize_marker: self.finalize_marker,
        }
    }

    fn restore_snapshot(&mut self, snapshot: Self::Snapshot) {
        snapshot
            .validate()
            .expect("counter snapshot payload must be validated before restore");
        self.tick = snapshot.tick;
        self.next_entity_id = snapshot.next_entity_id;
        self.primary_entity = snapshot.primary_entity;
        self.entity_order = snapshot.entities.iter().map(|entity| entity.id).collect();
        self.entities = snapshot
            .entities
            .iter()
            .map(|entity| {
                (
                    entity.id,
                    CounterEntity {
                        value: entity.value,
                        velocity: entity.velocity,
                    },
                )
            })
            .collect();
        self.transient = CounterTransientState {
            pending_delta: snapshot.pending_delta,
            pending_entropy: snapshot.pending_entropy,
        };
        self.entropy_budget = snapshot.entropy_budget;
        self.finalize_marker = snapshot.finalize_marker;
    }

    fn snapshot_schema_version() -> u32
    where
        Self: Sized,
    {
        2
    }

    fn snapshot_checksum(snapshot: &Self::Snapshot) -> u64
    where
        Self: Sized,
    {
        let words = Self::snapshot_checksum_words(snapshot);
        checksum_words(&words)
    }

    fn snapshot_tick(snapshot: &Self::Snapshot) -> Tick
    where
        Self: Sized,
    {
        snapshot.tick
    }
}
