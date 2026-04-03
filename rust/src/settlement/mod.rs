use crate::config::SimulationConfig;
use crate::core::{EngineError, Seed, Tick, checksum_words, mix64, tick_seed};
use crate::deterministic::DeterministicMap;
use crate::engine::SnapshotPolicy;
use crate::input::InputFrame;
use crate::phases::{Phase, TickContext};
use crate::replay::ReplayLog;
use crate::rng::Rng;
use crate::state::SimulationState;
use crate::validation::{
    StateValidator, ValidationCheckpoint, ValidationContext, ValidationPolicy,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResourceKind {
    Food,
    Wood,
}

impl ResourceKind {
    pub const ORDER: [Self; 2] = [Self::Food, Self::Wood];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Food => "food",
            Self::Wood => "wood",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "food" => Some(Self::Food),
            "wood" => Some(Self::Wood),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkerAllocation {
    pub farmers: u32,
    pub loggers: u32,
}

impl WorkerAllocation {
    pub const fn zero() -> Self {
        Self {
            farmers: 0,
            loggers: 0,
        }
    }

    pub const fn total_assigned(self) -> u32 {
        self.farmers.saturating_add(self.loggers)
    }

    pub fn idle_workers(self, population: u32) -> u32 {
        population.saturating_sub(self.total_assigned())
    }

    pub fn validate_against(self, population: u32) -> Result<(), String> {
        if self.total_assigned() > population {
            return Err(format!(
                "worker allocation exceeds population: assigned={}, population={population}",
                self.total_assigned()
            ));
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettlementStatus {
    Stable,
    FoodShortage,
    WoodShortage,
    FoodAndWoodShortage,
}

impl SettlementStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::FoodShortage => "food_shortage",
            Self::WoodShortage => "wood_shortage",
            Self::FoodAndWoodShortage => "food_and_wood_shortage",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "stable" => Some(Self::Stable),
            "food_shortage" => Some(Self::FoodShortage),
            "wood_shortage" => Some(Self::WoodShortage),
            "food_and_wood_shortage" => Some(Self::FoodAndWoodShortage),
            _ => None,
        }
    }

    pub const fn code(self) -> u64 {
        match self {
            Self::Stable => 0,
            Self::FoodShortage => 1,
            Self::WoodShortage => 2,
            Self::FoodAndWoodShortage => 3,
        }
    }

    pub const fn from_shortages(food_shortage: i64, wood_shortage: i64) -> Self {
        match (food_shortage > 0, wood_shortage > 0) {
            (false, false) => Self::Stable,
            (true, false) => Self::FoodShortage,
            (false, true) => Self::WoodShortage,
            (true, true) => Self::FoodAndWoodShortage,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementSimulationConfig {
    pub population: u32,
    pub initial_food: i64,
    pub initial_wood: i64,
    pub initial_allocation: WorkerAllocation,
    pub food_per_farmer: i64,
    pub wood_per_logger: i64,
    pub food_consumption_per_worker: i64,
    pub wood_consumption_per_tick: i64,
    pub snapshot_policy: SnapshotPolicy,
    pub validation_policy: ValidationPolicy,
    pub max_inventory: i64,
}

impl Default for SettlementSimulationConfig {
    fn default() -> Self {
        Self {
            population: 6,
            initial_food: 12,
            initial_wood: 8,
            initial_allocation: WorkerAllocation {
                farmers: 3,
                loggers: 2,
            },
            food_per_farmer: 3,
            wood_per_logger: 2,
            food_consumption_per_worker: 1,
            wood_consumption_per_tick: 2,
            snapshot_policy: SnapshotPolicy::Every { interval: 2 },
            validation_policy: ValidationPolicy::EveryPhase,
            max_inventory: 1_000,
        }
    }
}

impl SettlementSimulationConfig {
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.population == 0 {
            return Err(EngineError::ConfigMismatch {
                detail: "settlement population must be greater than 0".into(),
            });
        }

        self.initial_allocation
            .validate_against(self.population)
            .map_err(|detail| EngineError::ConfigMismatch { detail })?;

        for (label, value) in [
            ("initial_food", self.initial_food),
            ("initial_wood", self.initial_wood),
            ("food_per_farmer", self.food_per_farmer),
            ("wood_per_logger", self.wood_per_logger),
            (
                "food_consumption_per_worker",
                self.food_consumption_per_worker,
            ),
            ("wood_consumption_per_tick", self.wood_consumption_per_tick),
            ("max_inventory", self.max_inventory),
        ] {
            if value < 0 {
                return Err(EngineError::ConfigMismatch {
                    detail: format!("{label} must be non-negative, got {value}"),
                });
            }
        }

        for (label, value) in [
            ("initial_food", self.initial_food),
            ("initial_wood", self.initial_wood),
        ] {
            if value > self.max_inventory {
                return Err(EngineError::ConfigMismatch {
                    detail: format!(
                        "{label} exceeds max_inventory: value={value}, max_inventory={}",
                        self.max_inventory
                    ),
                });
            }
        }

        Ok(())
    }
}

impl SimulationConfig for SettlementSimulationConfig {
    fn snapshot_policy(&self) -> SnapshotPolicy {
        self.snapshot_policy
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettlementCommand {
    Hold,
    SetWorkerAllocation(WorkerAllocation),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementSnapshot {
    pub tick: Tick,
    pub population: u32,
    pub current_allocation: WorkerAllocation,
    pub pending_allocation_present: bool,
    pub pending_allocation: WorkerAllocation,
    pub food: i64,
    pub wood: i64,
    pub last_produced_food: i64,
    pub last_produced_wood: i64,
    pub last_consumed_food: i64,
    pub last_consumed_wood: i64,
    pub last_food_shortage: i64,
    pub last_wood_shortage: i64,
    pub total_food_produced: i64,
    pub total_wood_produced: i64,
    pub total_food_consumed: i64,
    pub total_wood_consumed: i64,
    pub total_food_shortage: i64,
    pub total_wood_shortage: i64,
    pub shortage_ticks: Tick,
    pub last_status: SettlementStatus,
    pub finalize_marker: u64,
}

impl SettlementSnapshot {
    pub fn validate(&self) -> Result<(), String> {
        self.current_allocation.validate_against(self.population)?;
        self.pending_allocation.validate_against(self.population)?;

        if !self.pending_allocation_present && self.pending_allocation != WorkerAllocation::zero() {
            return Err("pending allocation must be zeroed when not present".into());
        }

        for (label, value) in [
            ("food", self.food),
            ("wood", self.wood),
            ("last_produced_food", self.last_produced_food),
            ("last_produced_wood", self.last_produced_wood),
            ("last_consumed_food", self.last_consumed_food),
            ("last_consumed_wood", self.last_consumed_wood),
            ("last_food_shortage", self.last_food_shortage),
            ("last_wood_shortage", self.last_wood_shortage),
            ("total_food_produced", self.total_food_produced),
            ("total_wood_produced", self.total_wood_produced),
            ("total_food_consumed", self.total_food_consumed),
            ("total_wood_consumed", self.total_wood_consumed),
            ("total_food_shortage", self.total_food_shortage),
            ("total_wood_shortage", self.total_wood_shortage),
        ] {
            if value < 0 {
                return Err(format!("{label} must be non-negative, got {value}"));
            }
        }

        let expected_status =
            SettlementStatus::from_shortages(self.last_food_shortage, self.last_wood_shortage);
        if self.last_status != expected_status {
            return Err(format!(
                "last_status mismatch: expected `{}`, got `{}`",
                expected_status.as_str(),
                self.last_status.as_str()
            ));
        }

        if self.last_food_shortage > 0 && self.food != 0 {
            return Err(format!(
                "food must be 0 when food shortage is recorded, got {}",
                self.food
            ));
        }

        if self.last_wood_shortage > 0 && self.wood != 0 {
            return Err(format!(
                "wood must be 0 when wood shortage is recorded, got {}",
                self.wood
            ));
        }

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementRunSummary {
    pub final_tick: Tick,
    pub population: u32,
    pub final_allocation: WorkerAllocation,
    pub idle_workers: u32,
    pub final_food: i64,
    pub final_wood: i64,
    pub last_status: SettlementStatus,
    pub shortage_ticks: Tick,
    pub total_food_produced: i64,
    pub total_wood_produced: i64,
    pub total_food_consumed: i64,
    pub total_wood_consumed: i64,
    pub total_food_shortage: i64,
    pub total_wood_shortage: i64,
    pub final_checksum: u64,
}

impl SettlementRunSummary {
    pub fn from_snapshot(snapshot: &SettlementSnapshot) -> Result<Self, EngineError> {
        snapshot
            .validate()
            .map_err(|detail| EngineError::SnapshotDecode { detail })?;

        Ok(Self {
            final_tick: snapshot.tick,
            population: snapshot.population,
            final_allocation: snapshot.current_allocation,
            idle_workers: snapshot
                .current_allocation
                .idle_workers(snapshot.population),
            final_food: snapshot.food,
            final_wood: snapshot.wood,
            last_status: snapshot.last_status,
            shortage_ticks: snapshot.shortage_ticks,
            total_food_produced: snapshot.total_food_produced,
            total_wood_produced: snapshot.total_wood_produced,
            total_food_consumed: snapshot.total_food_consumed,
            total_wood_consumed: snapshot.total_wood_consumed,
            total_food_shortage: snapshot.total_food_shortage,
            total_wood_shortage: snapshot.total_wood_shortage,
            final_checksum: SettlementState::snapshot_checksum(snapshot),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementScenarioExpectation {
    pub final_allocation: WorkerAllocation,
    pub final_food: i64,
    pub final_wood: i64,
    pub final_status: SettlementStatus,
    pub shortage_ticks: Tick,
    pub total_food_produced: i64,
    pub total_wood_produced: i64,
    pub total_food_shortage: i64,
    pub total_wood_shortage: i64,
}

impl SettlementScenarioExpectation {
    pub fn verify(&self, summary: &SettlementRunSummary) -> Result<(), String> {
        if summary.final_allocation != self.final_allocation {
            return Err(format!(
                "final allocation mismatch: expected {:?}, got {:?}",
                self.final_allocation, summary.final_allocation
            ));
        }

        for (label, expected, actual) in [
            ("final_food", self.final_food, summary.final_food),
            ("final_wood", self.final_wood, summary.final_wood),
            (
                "total_food_produced",
                self.total_food_produced,
                summary.total_food_produced,
            ),
            (
                "total_wood_produced",
                self.total_wood_produced,
                summary.total_wood_produced,
            ),
            (
                "total_food_shortage",
                self.total_food_shortage,
                summary.total_food_shortage,
            ),
            (
                "total_wood_shortage",
                self.total_wood_shortage,
                summary.total_wood_shortage,
            ),
        ] {
            if expected != actual {
                return Err(format!(
                    "{label} mismatch: expected {expected}, got {actual}"
                ));
            }
        }

        if summary.shortage_ticks != self.shortage_ticks {
            return Err(format!(
                "shortage_ticks mismatch: expected {}, got {}",
                self.shortage_ticks, summary.shortage_ticks
            ));
        }

        if summary.last_status != self.final_status {
            return Err(format!(
                "final_status mismatch: expected `{}`, got `{}`",
                self.final_status.as_str(),
                summary.last_status.as_str()
            ));
        }

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementNamedScenario {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub config: SettlementSimulationConfig,
    pub seed: Seed,
    pub commands: Vec<SettlementCommand>,
    pub expected: SettlementScenarioExpectation,
}

impl SettlementNamedScenario {
    pub fn tick_count(&self) -> Tick {
        self.commands.len() as Tick
    }

    pub fn input_frames(&self) -> Vec<InputFrame<SettlementCommand>> {
        settlement_input_frames(&self.commands)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementState {
    tick: Tick,
    population: u32,
    current_allocation: WorkerAllocation,
    pending_allocation_present: bool,
    pending_allocation: WorkerAllocation,
    inventory: DeterministicMap<ResourceKind, i64>,
    last_production: DeterministicMap<ResourceKind, i64>,
    last_consumption: DeterministicMap<ResourceKind, i64>,
    last_shortage: DeterministicMap<ResourceKind, i64>,
    cumulative_production: DeterministicMap<ResourceKind, i64>,
    cumulative_consumption: DeterministicMap<ResourceKind, i64>,
    cumulative_shortage: DeterministicMap<ResourceKind, i64>,
    shortage_ticks: Tick,
    last_status: SettlementStatus,
    finalize_marker: u64,
}

impl SettlementState {
    pub fn from_config(config: &SettlementSimulationConfig) -> Result<Self, EngineError> {
        config.validate()?;
        Ok(Self {
            tick: 0,
            population: config.population,
            current_allocation: config.initial_allocation,
            pending_allocation_present: false,
            pending_allocation: WorkerAllocation::zero(),
            inventory: resource_map(config.initial_food, config.initial_wood),
            last_production: zero_resource_map(),
            last_consumption: zero_resource_map(),
            last_shortage: zero_resource_map(),
            cumulative_production: zero_resource_map(),
            cumulative_consumption: zero_resource_map(),
            cumulative_shortage: zero_resource_map(),
            shortage_ticks: 0,
            last_status: SettlementStatus::Stable,
            finalize_marker: 0,
        })
    }

    pub const fn population(&self) -> u32 {
        self.population
    }

    pub const fn current_allocation(&self) -> WorkerAllocation {
        self.current_allocation
    }

    pub fn pending_allocation(&self) -> Option<WorkerAllocation> {
        self.pending_allocation_present
            .then_some(self.pending_allocation)
    }

    pub fn resource(&self, resource: ResourceKind) -> i64 {
        resource_value(&self.inventory, resource)
    }

    pub fn last_produced(&self, resource: ResourceKind) -> i64 {
        resource_value(&self.last_production, resource)
    }

    pub fn last_consumed(&self, resource: ResourceKind) -> i64 {
        resource_value(&self.last_consumption, resource)
    }

    pub fn last_shortage(&self, resource: ResourceKind) -> i64 {
        resource_value(&self.last_shortage, resource)
    }

    pub fn total_produced(&self, resource: ResourceKind) -> i64 {
        resource_value(&self.cumulative_production, resource)
    }

    pub fn total_consumed(&self, resource: ResourceKind) -> i64 {
        resource_value(&self.cumulative_consumption, resource)
    }

    pub fn total_shortage(&self, resource: ResourceKind) -> i64 {
        resource_value(&self.cumulative_shortage, resource)
    }

    pub const fn shortage_ticks(&self) -> Tick {
        self.shortage_ticks
    }

    pub const fn last_status(&self) -> SettlementStatus {
        self.last_status
    }

    pub const fn finalize_marker(&self) -> u64 {
        self.finalize_marker
    }

    pub fn run_summary(&self) -> Result<SettlementRunSummary, EngineError> {
        SettlementRunSummary::from_snapshot(&self.snapshot())
    }

    pub(crate) fn reset_finalize_marker(&mut self) {
        self.finalize_marker = 0;
    }

    pub(crate) fn stage_allocation(
        &mut self,
        allocation: WorkerAllocation,
    ) -> Result<(), EngineError> {
        allocation
            .validate_against(self.population)
            .map_err(|detail| EngineError::ReplayLifecycle { detail })?;
        self.pending_allocation = allocation;
        self.pending_allocation_present = true;
        Ok(())
    }

    pub(crate) fn apply_pending_allocation(&mut self) {
        if self.pending_allocation_present {
            self.current_allocation = self.pending_allocation;
            self.pending_allocation_present = false;
            self.pending_allocation = WorkerAllocation::zero();
        }
    }

    pub(crate) fn produce(&mut self, config: &SettlementSimulationConfig) {
        let food_produced =
            i64::from(self.current_allocation.farmers).saturating_mul(config.food_per_farmer);
        let wood_produced =
            i64::from(self.current_allocation.loggers).saturating_mul(config.wood_per_logger);

        set_resource(&mut self.last_production, ResourceKind::Food, food_produced);
        set_resource(&mut self.last_production, ResourceKind::Wood, wood_produced);

        add_to_resource_map(&mut self.inventory, ResourceKind::Food, food_produced);
        add_to_resource_map(&mut self.inventory, ResourceKind::Wood, wood_produced);
        add_to_resource_map(
            &mut self.cumulative_production,
            ResourceKind::Food,
            food_produced,
        );
        add_to_resource_map(
            &mut self.cumulative_production,
            ResourceKind::Wood,
            wood_produced,
        );
    }

    pub(crate) fn consume(&mut self, config: &SettlementSimulationConfig) {
        let food_consumed = consume_resource(
            &mut self.inventory,
            &mut self.last_shortage,
            ResourceKind::Food,
            i64::from(self.population).saturating_mul(config.food_consumption_per_worker),
        );
        let wood_consumed = consume_resource(
            &mut self.inventory,
            &mut self.last_shortage,
            ResourceKind::Wood,
            config.wood_consumption_per_tick,
        );

        set_resource(
            &mut self.last_consumption,
            ResourceKind::Food,
            food_consumed,
        );
        set_resource(
            &mut self.last_consumption,
            ResourceKind::Wood,
            wood_consumed,
        );
        add_to_resource_map(
            &mut self.cumulative_consumption,
            ResourceKind::Food,
            food_consumed,
        );
        add_to_resource_map(
            &mut self.cumulative_consumption,
            ResourceKind::Wood,
            wood_consumed,
        );
        add_to_resource_map(
            &mut self.cumulative_shortage,
            ResourceKind::Food,
            resource_value(&self.last_shortage, ResourceKind::Food),
        );
        add_to_resource_map(
            &mut self.cumulative_shortage,
            ResourceKind::Wood,
            resource_value(&self.last_shortage, ResourceKind::Wood),
        );
    }

    pub(crate) fn resolve_shortage(&mut self) {
        self.last_status = SettlementStatus::from_shortages(
            resource_value(&self.last_shortage, ResourceKind::Food),
            resource_value(&self.last_shortage, ResourceKind::Wood),
        );

        if self.last_status != SettlementStatus::Stable {
            self.shortage_ticks = self.shortage_ticks.saturating_add(1);
        }
    }

    pub(crate) fn finalize(&mut self, marker: u64) {
        self.finalize_marker = marker;
    }

    pub fn preview_finalize_marker(&self, seed: Seed, tick: Tick) -> u64 {
        let mut snapshot = self.snapshot();
        snapshot.finalize_marker = 0;
        mix64(tick_seed(seed, tick) ^ Self::snapshot_checksum(&snapshot) ^ tick)
    }

    fn snapshot_checksum_words(snapshot: &SettlementSnapshot) -> Vec<u64> {
        vec![
            snapshot.tick,
            u64::from(snapshot.population),
            u64::from(snapshot.current_allocation.farmers),
            u64::from(snapshot.current_allocation.loggers),
            u64::from(snapshot.pending_allocation_present),
            u64::from(snapshot.pending_allocation.farmers),
            u64::from(snapshot.pending_allocation.loggers),
            snapshot.food as u64,
            snapshot.wood as u64,
            snapshot.last_produced_food as u64,
            snapshot.last_produced_wood as u64,
            snapshot.last_consumed_food as u64,
            snapshot.last_consumed_wood as u64,
            snapshot.last_food_shortage as u64,
            snapshot.last_wood_shortage as u64,
            snapshot.total_food_produced as u64,
            snapshot.total_wood_produced as u64,
            snapshot.total_food_consumed as u64,
            snapshot.total_wood_consumed as u64,
            snapshot.total_food_shortage as u64,
            snapshot.total_wood_shortage as u64,
            snapshot.shortage_ticks,
            snapshot.last_status.code(),
            snapshot.finalize_marker,
        ]
    }
}

impl SimulationState for SettlementState {
    type Snapshot = SettlementSnapshot;

    fn tick(&self) -> Tick {
        self.tick
    }

    fn set_tick(&mut self, tick: Tick) {
        self.tick = tick;
    }

    fn checksum(&self) -> u64 {
        Self::snapshot_checksum(&self.snapshot())
    }

    fn snapshot(&self) -> Self::Snapshot {
        SettlementSnapshot {
            tick: self.tick,
            population: self.population,
            current_allocation: self.current_allocation,
            pending_allocation_present: self.pending_allocation_present,
            pending_allocation: self.pending_allocation,
            food: self.resource(ResourceKind::Food),
            wood: self.resource(ResourceKind::Wood),
            last_produced_food: self.last_produced(ResourceKind::Food),
            last_produced_wood: self.last_produced(ResourceKind::Wood),
            last_consumed_food: self.last_consumed(ResourceKind::Food),
            last_consumed_wood: self.last_consumed(ResourceKind::Wood),
            last_food_shortage: self.last_shortage(ResourceKind::Food),
            last_wood_shortage: self.last_shortage(ResourceKind::Wood),
            total_food_produced: self.total_produced(ResourceKind::Food),
            total_wood_produced: self.total_produced(ResourceKind::Wood),
            total_food_consumed: self.total_consumed(ResourceKind::Food),
            total_wood_consumed: self.total_consumed(ResourceKind::Wood),
            total_food_shortage: self.total_shortage(ResourceKind::Food),
            total_wood_shortage: self.total_shortage(ResourceKind::Wood),
            shortage_ticks: self.shortage_ticks,
            last_status: self.last_status,
            finalize_marker: self.finalize_marker,
        }
    }

    fn restore_snapshot(&mut self, snapshot: Self::Snapshot) {
        snapshot
            .validate()
            .expect("settlement snapshot must be valid before restore");
        self.tick = snapshot.tick;
        self.population = snapshot.population;
        self.current_allocation = snapshot.current_allocation;
        self.pending_allocation_present = snapshot.pending_allocation_present;
        self.pending_allocation = snapshot.pending_allocation;
        self.inventory = resource_map(snapshot.food, snapshot.wood);
        self.last_production =
            resource_map(snapshot.last_produced_food, snapshot.last_produced_wood);
        self.last_consumption =
            resource_map(snapshot.last_consumed_food, snapshot.last_consumed_wood);
        self.last_shortage = resource_map(snapshot.last_food_shortage, snapshot.last_wood_shortage);
        self.cumulative_production =
            resource_map(snapshot.total_food_produced, snapshot.total_wood_produced);
        self.cumulative_consumption =
            resource_map(snapshot.total_food_consumed, snapshot.total_wood_consumed);
        self.cumulative_shortage =
            resource_map(snapshot.total_food_shortage, snapshot.total_wood_shortage);
        self.shortage_ticks = snapshot.shortage_ticks;
        self.last_status = snapshot.last_status;
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementStateValidator {
    seed: Seed,
    config: SettlementSimulationConfig,
}

impl SettlementStateValidator {
    pub fn new(seed: Seed, config: SettlementSimulationConfig) -> Self {
        Self { seed, config }
    }

    fn invariant_error(
        &self,
        context: ValidationContext,
        detail: impl Into<String>,
    ) -> EngineError {
        EngineError::InvariantViolation {
            tick: context.tick,
            checkpoint: context.checkpoint.as_str(),
            detail: detail.into(),
        }
    }

    fn expect_tick_alignment(
        &self,
        context: ValidationContext,
        state: &SettlementState,
    ) -> Result<(), EngineError> {
        let expected_state_tick = context.tick.saturating_sub(1);
        if state.tick() != expected_state_tick {
            return Err(self.invariant_error(
                context,
                format!(
                    "authoritative tick mismatch: expected {}, got {}",
                    expected_state_tick,
                    state.tick()
                ),
            ));
        }

        Ok(())
    }

    fn expect_allocation_valid(
        &self,
        context: ValidationContext,
        state: &SettlementState,
    ) -> Result<(), EngineError> {
        state
            .current_allocation
            .validate_against(state.population)
            .map_err(|detail| self.invariant_error(context, detail.clone()))?;

        if state.pending_allocation_present {
            state
                .pending_allocation
                .validate_against(state.population)
                .map_err(|detail| self.invariant_error(context, detail.clone()))?;
        } else if state.pending_allocation != WorkerAllocation::zero() {
            return Err(self.invariant_error(
                context,
                format!(
                    "pending allocation must be zeroed when absent, got {:?}",
                    state.pending_allocation
                ),
            ));
        }

        Ok(())
    }

    fn expect_inventory_limits(
        &self,
        context: ValidationContext,
        state: &SettlementState,
    ) -> Result<(), EngineError> {
        for resource in ResourceKind::ORDER {
            let value = state.resource(resource);
            if value < 0 {
                return Err(self.invariant_error(
                    context,
                    format!(
                        "{} inventory must be non-negative, got {value}",
                        resource.as_str()
                    ),
                ));
            }

            if value > self.config.max_inventory {
                return Err(self.invariant_error(
                    context,
                    format!(
                        "{} inventory exceeds max_inventory: value={value}, max_inventory={}",
                        resource.as_str(),
                        self.config.max_inventory
                    ),
                ));
            }
        }

        Ok(())
    }

    fn expect_non_negative_metrics(
        &self,
        context: ValidationContext,
        state: &SettlementState,
    ) -> Result<(), EngineError> {
        for (label, value) in [
            (
                "last_produced_food",
                state.last_produced(ResourceKind::Food),
            ),
            (
                "last_produced_wood",
                state.last_produced(ResourceKind::Wood),
            ),
            (
                "last_consumed_food",
                state.last_consumed(ResourceKind::Food),
            ),
            (
                "last_consumed_wood",
                state.last_consumed(ResourceKind::Wood),
            ),
            (
                "last_food_shortage",
                state.last_shortage(ResourceKind::Food),
            ),
            (
                "last_wood_shortage",
                state.last_shortage(ResourceKind::Wood),
            ),
            (
                "total_food_produced",
                state.total_produced(ResourceKind::Food),
            ),
            (
                "total_wood_produced",
                state.total_produced(ResourceKind::Wood),
            ),
            (
                "total_food_consumed",
                state.total_consumed(ResourceKind::Food),
            ),
            (
                "total_wood_consumed",
                state.total_consumed(ResourceKind::Wood),
            ),
            (
                "total_food_shortage",
                state.total_shortage(ResourceKind::Food),
            ),
            (
                "total_wood_shortage",
                state.total_shortage(ResourceKind::Wood),
            ),
        ] {
            if value < 0 {
                return Err(self.invariant_error(
                    context,
                    format!("{label} must be non-negative, got {value}"),
                ));
            }
        }

        Ok(())
    }

    fn expect_shortage_consistency(
        &self,
        context: ValidationContext,
        state: &SettlementState,
    ) -> Result<(), EngineError> {
        let expected_status = SettlementStatus::from_shortages(
            state.last_shortage(ResourceKind::Food),
            state.last_shortage(ResourceKind::Wood),
        );
        if state.last_status != expected_status {
            return Err(self.invariant_error(
                context,
                format!(
                    "last_status mismatch: expected `{}`, got `{}`",
                    expected_status.as_str(),
                    state.last_status.as_str()
                ),
            ));
        }

        if state.last_shortage(ResourceKind::Food) > 0 && state.resource(ResourceKind::Food) != 0 {
            return Err(self.invariant_error(
                context,
                format!(
                    "food inventory must be 0 when shortage is recorded, got {}",
                    state.resource(ResourceKind::Food)
                ),
            ));
        }

        if state.last_shortage(ResourceKind::Wood) > 0 && state.resource(ResourceKind::Wood) != 0 {
            return Err(self.invariant_error(
                context,
                format!(
                    "wood inventory must be 0 when shortage is recorded, got {}",
                    state.resource(ResourceKind::Wood)
                ),
            ));
        }

        Ok(())
    }

    fn expect_marker_zero(
        &self,
        context: ValidationContext,
        state: &SettlementState,
    ) -> Result<(), EngineError> {
        if state.finalize_marker != 0 {
            return Err(self.invariant_error(
                context,
                format!(
                    "finalize_marker must be cleared, got {}",
                    state.finalize_marker
                ),
            ));
        }

        Ok(())
    }

    fn expect_previous_marker(
        &self,
        context: ValidationContext,
        state: &SettlementState,
    ) -> Result<(), EngineError> {
        if state.tick == 0 {
            return self.expect_marker_zero(context, state);
        }

        if state.finalize_marker == 0 {
            return Err(self.invariant_error(
                context,
                format!("finalize_marker missing for completed tick {}", state.tick),
            ));
        }

        Ok(())
    }

    fn expect_current_marker(
        &self,
        context: ValidationContext,
        state: &SettlementState,
    ) -> Result<(), EngineError> {
        let expected_marker = state.preview_finalize_marker(self.seed, context.tick);
        if state.finalize_marker != expected_marker {
            return Err(self.invariant_error(
                context,
                format!(
                    "finalize_marker mismatch for tick {}: expected {}, got {}",
                    context.tick, expected_marker, state.finalize_marker
                ),
            ));
        }

        Ok(())
    }

    fn expect_pending_cleared(
        &self,
        context: ValidationContext,
        state: &SettlementState,
    ) -> Result<(), EngineError> {
        if state.pending_allocation_present || state.pending_allocation != WorkerAllocation::zero()
        {
            return Err(self.invariant_error(
                context,
                format!(
                    "pending allocation must be cleared after phase boundary, got present={} allocation={:?}",
                    state.pending_allocation_present,
                    state.pending_allocation
                ),
            ));
        }

        Ok(())
    }
}

impl StateValidator<SettlementState> for SettlementStateValidator {
    fn validate(
        &self,
        context: ValidationContext,
        state: &SettlementState,
    ) -> Result<(), EngineError> {
        self.expect_tick_alignment(context, state)?;
        self.expect_allocation_valid(context, state)?;
        self.expect_inventory_limits(context, state)?;
        self.expect_non_negative_metrics(context, state)?;
        self.expect_shortage_consistency(context, state)?;

        match context.checkpoint {
            ValidationCheckpoint::BeforeTickBegin => {
                self.expect_pending_cleared(context, state)?;
                self.expect_previous_marker(context, state)?;
            }
            ValidationCheckpoint::AfterInputApplied => {
                self.expect_marker_zero(context, state)?;
            }
            ValidationCheckpoint::AfterSimulationGroup => {
                self.expect_marker_zero(context, state)?;
                self.expect_pending_cleared(context, state)?;
            }
            ValidationCheckpoint::AfterFinalize => {
                self.expect_pending_cleared(context, state)?;
                self.expect_current_marker(context, state)?;
            }
        }

        Ok(())
    }
}

#[derive(Default)]
pub struct ResetSettlementFinalizeMarkerPhase;

impl<R, L> Phase<SettlementState, SettlementCommand, R, L> for ResetSettlementFinalizeMarkerPhase
where
    R: Rng,
    L: ReplayLog<SettlementCommand, SettlementSnapshot>,
{
    fn name(&self) -> &'static str {
        "reset_settlement_finalize_marker"
    }

    fn run(
        &mut self,
        ctx: &mut TickContext<'_, SettlementState, SettlementCommand, R, L>,
    ) -> Result<(), EngineError> {
        ctx.state_mut().reset_finalize_marker();
        Ok(())
    }
}

#[derive(Default)]
pub struct StageSettlementCommandPhase;

impl<R, L> Phase<SettlementState, SettlementCommand, R, L> for StageSettlementCommandPhase
where
    R: Rng,
    L: ReplayLog<SettlementCommand, SettlementSnapshot>,
{
    fn name(&self) -> &'static str {
        "stage_settlement_command"
    }

    fn run(
        &mut self,
        ctx: &mut TickContext<'_, SettlementState, SettlementCommand, R, L>,
    ) -> Result<(), EngineError> {
        match &ctx.frame().command {
            SettlementCommand::Hold => Ok(()),
            SettlementCommand::SetWorkerAllocation(allocation) => {
                ctx.state_mut().stage_allocation(*allocation)
            }
        }
    }
}

#[derive(Default)]
pub struct ApplySettlementAllocationPhase;

impl<R, L> Phase<SettlementState, SettlementCommand, R, L> for ApplySettlementAllocationPhase
where
    R: Rng,
    L: ReplayLog<SettlementCommand, SettlementSnapshot>,
{
    fn name(&self) -> &'static str {
        "apply_settlement_allocation"
    }

    fn run(
        &mut self,
        ctx: &mut TickContext<'_, SettlementState, SettlementCommand, R, L>,
    ) -> Result<(), EngineError> {
        ctx.state_mut().apply_pending_allocation();
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct SettlementProductionPhase {
    config: SettlementSimulationConfig,
}

impl SettlementProductionPhase {
    pub fn new(config: SettlementSimulationConfig) -> Self {
        Self { config }
    }
}

impl<R, L> Phase<SettlementState, SettlementCommand, R, L> for SettlementProductionPhase
where
    R: Rng,
    L: ReplayLog<SettlementCommand, SettlementSnapshot>,
{
    fn name(&self) -> &'static str {
        "settlement_production"
    }

    fn run(
        &mut self,
        ctx: &mut TickContext<'_, SettlementState, SettlementCommand, R, L>,
    ) -> Result<(), EngineError> {
        ctx.state_mut().produce(&self.config);
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct SettlementConsumptionPhase {
    config: SettlementSimulationConfig,
}

impl SettlementConsumptionPhase {
    pub fn new(config: SettlementSimulationConfig) -> Self {
        Self { config }
    }
}

impl<R, L> Phase<SettlementState, SettlementCommand, R, L> for SettlementConsumptionPhase
where
    R: Rng,
    L: ReplayLog<SettlementCommand, SettlementSnapshot>,
{
    fn name(&self) -> &'static str {
        "settlement_consumption"
    }

    fn run(
        &mut self,
        ctx: &mut TickContext<'_, SettlementState, SettlementCommand, R, L>,
    ) -> Result<(), EngineError> {
        ctx.state_mut().consume(&self.config);
        Ok(())
    }
}

#[derive(Default)]
pub struct ResolveSettlementStatusPhase;

impl<R, L> Phase<SettlementState, SettlementCommand, R, L> for ResolveSettlementStatusPhase
where
    R: Rng,
    L: ReplayLog<SettlementCommand, SettlementSnapshot>,
{
    fn name(&self) -> &'static str {
        "resolve_settlement_status"
    }

    fn run(
        &mut self,
        ctx: &mut TickContext<'_, SettlementState, SettlementCommand, R, L>,
    ) -> Result<(), EngineError> {
        ctx.state_mut().resolve_shortage();
        Ok(())
    }
}

#[derive(Default)]
pub struct FinalizeSettlementPhase;

impl<R, L> Phase<SettlementState, SettlementCommand, R, L> for FinalizeSettlementPhase
where
    R: Rng,
    L: ReplayLog<SettlementCommand, SettlementSnapshot>,
{
    fn name(&self) -> &'static str {
        "finalize_settlement"
    }

    fn run(
        &mut self,
        ctx: &mut TickContext<'_, SettlementState, SettlementCommand, R, L>,
    ) -> Result<(), EngineError> {
        let marker = mix64(ctx.tick_seed() ^ ctx.state().checksum() ^ ctx.tick());
        ctx.state_mut().finalize(marker);
        Ok(())
    }
}

pub fn settlement_input_frames(
    commands: &[SettlementCommand],
) -> Vec<InputFrame<SettlementCommand>> {
    commands
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, command)| InputFrame::new(index as Tick + 1, command))
        .collect()
}

fn zero_resource_map() -> DeterministicMap<ResourceKind, i64> {
    resource_map(0, 0)
}

fn resource_map(food: i64, wood: i64) -> DeterministicMap<ResourceKind, i64> {
    [(ResourceKind::Food, food), (ResourceKind::Wood, wood)]
        .into_iter()
        .collect()
}

fn resource_value(map: &DeterministicMap<ResourceKind, i64>, resource: ResourceKind) -> i64 {
    *map.get(&resource)
        .expect("settlement resource map must contain food and wood")
}

fn set_resource(map: &mut DeterministicMap<ResourceKind, i64>, resource: ResourceKind, value: i64) {
    map.insert(resource, value);
}

fn add_to_resource_map(
    map: &mut DeterministicMap<ResourceKind, i64>,
    resource: ResourceKind,
    amount: i64,
) {
    let next = resource_value(map, resource).saturating_add(amount);
    set_resource(map, resource, next);
}

fn consume_resource(
    inventory: &mut DeterministicMap<ResourceKind, i64>,
    shortages: &mut DeterministicMap<ResourceKind, i64>,
    resource: ResourceKind,
    required: i64,
) -> i64 {
    let available = resource_value(inventory, resource);
    let actual = available.min(required);
    let shortage = required.saturating_sub(actual);
    set_resource(inventory, resource, available.saturating_sub(actual));
    set_resource(shortages, resource, shortage);
    actual
}

pub fn settlement_demo_scenarios() -> Vec<SettlementNamedScenario> {
    let balanced = SettlementSimulationConfig {
        initial_food: 18,
        initial_wood: 10,
        population: 6,
        initial_allocation: WorkerAllocation {
            farmers: 3,
            loggers: 2,
        },
        food_per_farmer: 3,
        wood_per_logger: 2,
        food_consumption_per_worker: 1,
        wood_consumption_per_tick: 2,
        ..SettlementSimulationConfig::default()
    };

    let shortage = SettlementSimulationConfig {
        initial_food: 0,
        initial_wood: 12,
        initial_allocation: WorkerAllocation {
            farmers: 1,
            loggers: 4,
        },
        food_per_farmer: 2,
        wood_per_logger: 3,
        ..balanced.clone()
    };

    let wood_surplus = SettlementSimulationConfig {
        population: 4,
        initial_food: 12,
        initial_wood: 8,
        initial_allocation: WorkerAllocation {
            farmers: 2,
            loggers: 2,
        },
        food_per_farmer: 3,
        wood_per_logger: 4,
        wood_consumption_per_tick: 1,
        ..balanced.clone()
    };

    let recovery = SettlementSimulationConfig {
        population: 5,
        initial_food: 3,
        initial_wood: 8,
        initial_allocation: WorkerAllocation {
            farmers: 1,
            loggers: 3,
        },
        food_per_farmer: 2,
        wood_per_logger: 3,
        wood_consumption_per_tick: 1,
        ..balanced.clone()
    };

    vec![
        SettlementNamedScenario {
            id: "balanced_settlement",
            title: "Balanced settlement",
            description: "Food and wood both stay ahead of consumption with a stable worker split.",
            config: balanced,
            seed: 101,
            commands: vec![SettlementCommand::Hold; 6],
            expected: SettlementScenarioExpectation {
                final_allocation: WorkerAllocation {
                    farmers: 3,
                    loggers: 2,
                },
                final_food: 36,
                final_wood: 22,
                final_status: SettlementStatus::Stable,
                shortage_ticks: 0,
                total_food_produced: 54,
                total_wood_produced: 24,
                total_food_shortage: 0,
                total_wood_shortage: 0,
            },
        },
        SettlementNamedScenario {
            id: "food_shortage",
            title: "Food shortage",
            description: "Too many workers are assigned to wood, so food reserves collapse deterministically.",
            config: shortage,
            seed: 202,
            commands: vec![SettlementCommand::Hold; 6],
            expected: SettlementScenarioExpectation {
                final_allocation: WorkerAllocation {
                    farmers: 1,
                    loggers: 4,
                },
                final_food: 0,
                final_wood: 72,
                final_status: SettlementStatus::FoodShortage,
                shortage_ticks: 6,
                total_food_produced: 12,
                total_wood_produced: 72,
                total_food_shortage: 24,
                total_wood_shortage: 0,
            },
        },
        SettlementNamedScenario {
            id: "wood_surplus",
            title: "Wood surplus",
            description: "A moderate settlement keeps food positive while wood stockpiles grow every tick.",
            config: wood_surplus,
            seed: 303,
            commands: vec![SettlementCommand::Hold; 6],
            expected: SettlementScenarioExpectation {
                final_allocation: WorkerAllocation {
                    farmers: 2,
                    loggers: 2,
                },
                final_food: 24,
                final_wood: 50,
                final_status: SettlementStatus::Stable,
                shortage_ticks: 0,
                total_food_produced: 36,
                total_wood_produced: 48,
                total_food_shortage: 0,
                total_wood_shortage: 0,
            },
        },
        SettlementNamedScenario {
            id: "recovery_after_reallocation",
            title: "Recovery after reallocation",
            description: "A food-starved settlement recovers after workers are reallocated toward farming.",
            config: recovery,
            seed: 404,
            commands: vec![
                SettlementCommand::Hold,
                SettlementCommand::Hold,
                SettlementCommand::SetWorkerAllocation(WorkerAllocation {
                    farmers: 3,
                    loggers: 1,
                }),
                SettlementCommand::Hold,
                SettlementCommand::Hold,
                SettlementCommand::Hold,
            ],
            expected: SettlementScenarioExpectation {
                final_allocation: WorkerAllocation {
                    farmers: 3,
                    loggers: 1,
                },
                final_food: 4,
                final_wood: 32,
                final_status: SettlementStatus::Stable,
                shortage_ticks: 1,
                total_food_produced: 28,
                total_wood_produced: 30,
                total_food_shortage: 3,
                total_wood_shortage: 0,
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        ResourceKind, SettlementRunSummary, SettlementSimulationConfig, SettlementSnapshot,
        SettlementState, SettlementStatus, WorkerAllocation, settlement_demo_scenarios,
        settlement_input_frames,
    };
    use crate::engine::SnapshotPolicy;
    use crate::state::SimulationState;

    #[test]
    fn settlement_snapshot_validation_rejects_inconsistent_status() {
        let snapshot = SettlementSnapshot {
            tick: 1,
            population: 4,
            current_allocation: WorkerAllocation {
                farmers: 2,
                loggers: 1,
            },
            pending_allocation_present: false,
            pending_allocation: WorkerAllocation::zero(),
            food: 4,
            wood: 3,
            last_produced_food: 2,
            last_produced_wood: 1,
            last_consumed_food: 4,
            last_consumed_wood: 1,
            last_food_shortage: 1,
            last_wood_shortage: 0,
            total_food_produced: 2,
            total_wood_produced: 1,
            total_food_consumed: 4,
            total_wood_consumed: 1,
            total_food_shortage: 1,
            total_wood_shortage: 0,
            shortage_ticks: 1,
            last_status: SettlementStatus::Stable,
            finalize_marker: 0,
        };

        assert!(
            snapshot
                .validate()
                .expect_err("status mismatch should fail")
                .contains("last_status mismatch")
        );
    }

    #[test]
    fn settlement_state_summary_tracks_resource_totals() {
        let config = SettlementSimulationConfig {
            snapshot_policy: SnapshotPolicy::Never,
            ..SettlementSimulationConfig::default()
        };
        let mut state = SettlementState::from_config(&config).expect("config should be valid");
        state.produce(&config);
        state.consume(&config);
        state.resolve_shortage();
        state.finalize(77);
        state.set_tick(1);

        let summary = SettlementRunSummary::from_snapshot(&state.snapshot())
            .expect("summary should derive from snapshot");

        assert_eq!(summary.final_tick, 1);
        assert_eq!(summary.final_food, 15);
        assert_eq!(summary.final_wood, 10);
        assert_eq!(summary.total_food_produced, 9);
        assert_eq!(summary.total_wood_produced, 4);
        assert_eq!(summary.last_status, SettlementStatus::Stable);
    }

    #[test]
    fn settlement_demo_scenarios_cover_shortage_and_recovery_cases() {
        let scenarios = settlement_demo_scenarios();

        assert_eq!(scenarios.len(), 4);
        assert_eq!(scenarios[0].id, "balanced_settlement");
        assert_eq!(scenarios[1].config.initial_allocation.farmers, 1);
        assert!(matches!(
            scenarios[3].commands[2],
            super::SettlementCommand::SetWorkerAllocation(_)
        ));
    }

    #[test]
    fn settlement_state_snapshot_roundtrip_preserves_resources() {
        let config = SettlementSimulationConfig::default();
        let state = SettlementState::from_config(&config).expect("config should be valid");
        let snapshot = state.snapshot();
        let mut restored = SettlementState::from_config(&config).expect("config should be valid");
        restored.restore_snapshot(snapshot.clone());

        assert_eq!(restored.snapshot(), snapshot);
        assert_eq!(restored.resource(ResourceKind::Food), config.initial_food);
        assert_eq!(restored.resource(ResourceKind::Wood), config.initial_wood);
    }

    #[test]
    fn settlement_input_frames_assign_monotonic_ticks() {
        let frames = settlement_input_frames(&[
            super::SettlementCommand::Hold,
            super::SettlementCommand::Hold,
            super::SettlementCommand::SetWorkerAllocation(WorkerAllocation {
                farmers: 3,
                loggers: 1,
            }),
        ]);

        assert_eq!(frames[0].tick, 1);
        assert_eq!(frames[1].tick, 2);
        assert_eq!(frames[2].tick, 3);
    }
}
