use crate::core::{Seed, Tick};
use crate::state::SimulationState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidationPolicy {
    TickBoundary,
    EveryPhase,
}

impl ValidationPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TickBoundary => "tick_boundary",
            Self::EveryPhase => "every_phase",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "tick_boundary" => Some(Self::TickBoundary),
            "every_phase" => Some(Self::EveryPhase),
            _ => None,
        }
    }

    pub const fn should_validate(self, checkpoint: ValidationCheckpoint) -> bool {
        match self {
            Self::TickBoundary => matches!(
                checkpoint,
                ValidationCheckpoint::BeforeTickBegin | ValidationCheckpoint::AfterFinalize
            ),
            Self::EveryPhase => true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidationCheckpoint {
    BeforeTickBegin,
    AfterInputApplied,
    AfterSimulationGroup,
    AfterFinalize,
}

impl ValidationCheckpoint {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BeforeTickBegin => "before_tick_begin",
            Self::AfterInputApplied => "after_input_applied",
            Self::AfterSimulationGroup => "after_simulation_group",
            Self::AfterFinalize => "after_finalize",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "before_tick_begin" => Some(Self::BeforeTickBegin),
            "after_input_applied" => Some(Self::AfterInputApplied),
            "after_simulation_group" => Some(Self::AfterSimulationGroup),
            "after_finalize" => Some(Self::AfterFinalize),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidationContext {
    pub checkpoint: ValidationCheckpoint,
    pub tick: Tick,
    pub tick_seed: Seed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationSummary {
    pub checkpoint: ValidationCheckpoint,
    pub state_tick: Tick,
    pub state_digest: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateDigestProgression {
    pub tick: Tick,
    pub checkpoints: Vec<ValidationSummary>,
}

pub trait StateValidator<S>: Clone
where
    S: SimulationState,
{
    fn validate(
        &self,
        context: ValidationContext,
        state: &S,
    ) -> Result<(), crate::core::EngineError>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoopStateValidator;

impl<S> StateValidator<S> for NoopStateValidator
where
    S: SimulationState,
{
    fn validate(
        &self,
        _context: ValidationContext,
        _state: &S,
    ) -> Result<(), crate::core::EngineError> {
        Ok(())
    }
}
