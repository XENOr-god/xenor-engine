use std::marker::PhantomData;

use crate::core::{Seed, Tick, fork_seed};
use crate::input::{Command, InputFrame};
use crate::replay::ReplayLog;
use crate::rng::Rng;
use crate::state::SimulationState;

pub struct TickContext<'a, S, C, R, L>
where
    S: SimulationState,
    C: Command,
    R: Rng,
    L: ReplayLog<C, S::Snapshot>,
{
    pub base_seed: Seed,
    pub tick_seed: Seed,
    pub tick: Tick,
    pub frame: &'a InputFrame<C>,
    pub state: &'a mut S,
    pub replay: &'a mut L,
    pub(crate) _marker: PhantomData<R>,
}

impl<'a, S, C, R, L> TickContext<'a, S, C, R, L>
where
    S: SimulationState,
    C: Command,
    R: Rng,
    L: ReplayLog<C, S::Snapshot>,
{
    pub fn new(
        base_seed: Seed,
        tick_seed: Seed,
        tick: Tick,
        frame: &'a InputFrame<C>,
        state: &'a mut S,
        replay: &'a mut L,
    ) -> Self {
        Self {
            base_seed,
            tick_seed,
            tick,
            frame,
            state,
            replay,
            _marker: PhantomData,
        }
    }

    pub fn rng_for(&self, stream: &'static str) -> R {
        R::from_seed(fork_seed(self.tick_seed, stream))
    }
}

pub trait Phase<S, C, R, L>
where
    S: SimulationState,
    C: Command,
    R: Rng,
    L: ReplayLog<C, S::Snapshot>,
{
    fn name(&self) -> &'static str;

    fn run(
        &mut self,
        ctx: &mut TickContext<'_, S, C, R, L>,
    ) -> Result<(), crate::core::EngineError>;
}
