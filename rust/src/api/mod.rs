use crate::core::{EngineError, Seed, mix64};
use crate::engine::{DeterministicEngine, Engine, SnapshotPolicy};
use crate::input::{Command, InputFrame};
use crate::phases::{Phase, TickContext};
use crate::replay::InMemoryReplayLog;
use crate::rng::{Rng, SplitMix64};
use crate::scheduler::{FixedScheduler, PhaseGroup};
use crate::state::{CounterSnapshot, CounterState, SimulationState};

pub trait EngineApi<C: Command>: Engine<C> {
    fn snapshot(&self) -> <Self::State as SimulationState>::Snapshot {
        self.state().snapshot()
    }
}

impl<T, C> EngineApi<C> for T
where
    T: Engine<C>,
    C: Command,
{
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CounterCommand {
    pub delta: i64,
    pub consume_entropy: bool,
}

pub type CounterReplayLog = InMemoryReplayLog<CounterCommand, CounterSnapshot>;
pub type CounterScheduler =
    FixedScheduler<CounterState, CounterCommand, SplitMix64, CounterReplayLog>;
pub type CounterEngine = DeterministicEngine<
    CounterState,
    CounterCommand,
    SplitMix64,
    CounterReplayLog,
    CounterScheduler,
>;

#[derive(Default)]
pub struct ResetFinalizeMarkerPhase;

impl Phase<CounterState, CounterCommand, SplitMix64, CounterReplayLog>
    for ResetFinalizeMarkerPhase
{
    fn name(&self) -> &'static str {
        "reset_finalize_marker"
    }

    fn run(
        &mut self,
        ctx: &mut TickContext<'_, CounterState, CounterCommand, SplitMix64, CounterReplayLog>,
    ) -> Result<(), EngineError> {
        ctx.state.reset_finalize_marker();
        Ok(())
    }
}

#[derive(Default)]
pub struct ApplyCounterInputPhase;

impl Phase<CounterState, CounterCommand, SplitMix64, CounterReplayLog> for ApplyCounterInputPhase {
    fn name(&self) -> &'static str {
        "apply_counter_input"
    }

    fn run(
        &mut self,
        ctx: &mut TickContext<'_, CounterState, CounterCommand, SplitMix64, CounterReplayLog>,
    ) -> Result<(), EngineError> {
        let entropy = if ctx.frame.command.consume_entropy {
            let mut rng = ctx.rng_for(self.name());
            rng.next_u64() & 0xff
        } else {
            0
        };

        ctx.state.stage_input(ctx.frame.command.delta, entropy);
        Ok(())
    }
}

#[derive(Default)]
pub struct SimulateCounterPhase;

impl Phase<CounterState, CounterCommand, SplitMix64, CounterReplayLog> for SimulateCounterPhase {
    fn name(&self) -> &'static str {
        "simulate_counter"
    }

    fn run(
        &mut self,
        ctx: &mut TickContext<'_, CounterState, CounterCommand, SplitMix64, CounterReplayLog>,
    ) -> Result<(), EngineError> {
        ctx.state.simulate();
        Ok(())
    }
}

#[derive(Default)]
pub struct SettleCounterPhase;

impl Phase<CounterState, CounterCommand, SplitMix64, CounterReplayLog> for SettleCounterPhase {
    fn name(&self) -> &'static str {
        "settle_counter"
    }

    fn run(
        &mut self,
        ctx: &mut TickContext<'_, CounterState, CounterCommand, SplitMix64, CounterReplayLog>,
    ) -> Result<(), EngineError> {
        ctx.state.settle();
        Ok(())
    }
}

#[derive(Default)]
pub struct FinalizeCounterPhase;

impl Phase<CounterState, CounterCommand, SplitMix64, CounterReplayLog> for FinalizeCounterPhase {
    fn name(&self) -> &'static str {
        "finalize_counter"
    }

    fn run(
        &mut self,
        ctx: &mut TickContext<'_, CounterState, CounterCommand, SplitMix64, CounterReplayLog>,
    ) -> Result<(), EngineError> {
        let marker = mix64(ctx.tick_seed ^ ctx.state.checksum() ^ ctx.tick);
        ctx.state.finalize(marker);
        Ok(())
    }
}

pub(crate) fn default_counter_scheduler() -> CounterScheduler {
    let mut scheduler = CounterScheduler::new();
    scheduler.add_phase(PhaseGroup::PreInput, ResetFinalizeMarkerPhase);
    scheduler.add_phase(PhaseGroup::Input, ApplyCounterInputPhase);
    scheduler.add_phase(PhaseGroup::Simulation, SimulateCounterPhase);
    scheduler.add_phase(PhaseGroup::PostSimulation, SettleCounterPhase);
    scheduler.add_phase(PhaseGroup::Finalize, FinalizeCounterPhase);
    scheduler
}

#[cfg(test)]
pub(crate) fn reordered_counter_scheduler() -> CounterScheduler {
    let mut scheduler = CounterScheduler::new();
    scheduler.add_phase(PhaseGroup::PreInput, ResetFinalizeMarkerPhase);
    scheduler.add_phase(PhaseGroup::Input, SimulateCounterPhase);
    scheduler.add_phase(PhaseGroup::Simulation, ApplyCounterInputPhase);
    scheduler.add_phase(PhaseGroup::PostSimulation, SettleCounterPhase);
    scheduler.add_phase(PhaseGroup::Finalize, FinalizeCounterPhase);
    scheduler
}

pub(crate) fn build_counter_engine(
    seed: Seed,
    scheduler: CounterScheduler,
    snapshot_policy: SnapshotPolicy,
) -> CounterEngine {
    DeterministicEngine::new(
        seed,
        CounterState::default(),
        scheduler,
        CounterReplayLog::default(),
    )
    .with_snapshot_policy(snapshot_policy)
}

pub fn counter_engine_with_policy(seed: Seed, snapshot_policy: SnapshotPolicy) -> CounterEngine {
    build_counter_engine(seed, default_counter_scheduler(), snapshot_policy)
}

pub fn minimal_counter_engine(seed: Seed) -> CounterEngine {
    counter_engine_with_policy(seed, SnapshotPolicy::Every { interval: 1 })
}

pub fn one_tick_counter_snapshot(seed: Seed, delta: i64) -> CounterSnapshot {
    let mut engine = minimal_counter_engine(seed);
    engine
        .tick(InputFrame::new(
            1,
            CounterCommand {
                delta,
                consume_entropy: true,
            },
        ))
        .expect("minimal counter engine should tick deterministically");
    engine.snapshot()
}
