use crate::core::EngineError;
use crate::input::Command;
use crate::phases::Phase;
use crate::replay::ReplayLog;
use crate::rng::Rng;
use crate::state::SimulationState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PhaseGroup {
    PreInput,
    Input,
    Simulation,
    PostSimulation,
    Finalize,
}

impl PhaseGroup {
    pub const ALL: [Self; 5] = [
        Self::PreInput,
        Self::Input,
        Self::Simulation,
        Self::PostSimulation,
        Self::Finalize,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PreInput => "PreInput",
            Self::Input => "Input",
            Self::Simulation => "Simulation",
            Self::PostSimulation => "PostSimulation",
            Self::Finalize => "Finalize",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "PreInput" => Some(Self::PreInput),
            "Input" => Some(Self::Input),
            "Simulation" => Some(Self::Simulation),
            "PostSimulation" => Some(Self::PostSimulation),
            "Finalize" => Some(Self::Finalize),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhaseDescriptor {
    pub ordinal: usize,
    pub group: PhaseGroup,
    pub name: &'static str,
}

pub trait Scheduler<S, C, R, L>
where
    S: SimulationState,
    C: Command,
    R: Rng,
    L: ReplayLog<C, S::Snapshot>,
{
    fn visit_phases(
        &mut self,
        visitor: &mut dyn FnMut(
            PhaseDescriptor,
            &mut dyn Phase<S, C, R, L>,
        ) -> Result<(), EngineError>,
    ) -> Result<(), EngineError>;

    fn phase_plan(&self) -> Vec<PhaseDescriptor>;

    fn phase_order(&self) -> Vec<&'static str> {
        self.phase_plan()
            .into_iter()
            .map(|entry| entry.name)
            .collect()
    }

    fn group_members(&self, group: PhaseGroup) -> Vec<&'static str> {
        self.phase_plan()
            .into_iter()
            .filter(|entry| entry.group == group)
            .map(|entry| entry.name)
            .collect()
    }
}

struct PhaseEntry<S, C, R, L>
where
    S: SimulationState,
    C: Command,
    R: Rng,
    L: ReplayLog<C, S::Snapshot>,
{
    group: PhaseGroup,
    insertion_order: usize,
    phase: Box<dyn Phase<S, C, R, L>>,
}

pub struct FixedScheduler<S, C, R, L>
where
    S: SimulationState,
    C: Command,
    R: Rng,
    L: ReplayLog<C, S::Snapshot>,
{
    phases: Vec<PhaseEntry<S, C, R, L>>,
    next_insertion_order: usize,
}

impl<S, C, R, L> FixedScheduler<S, C, R, L>
where
    S: SimulationState,
    C: Command,
    R: Rng,
    L: ReplayLog<C, S::Snapshot>,
{
    pub fn new() -> Self {
        Self {
            phases: Vec::new(),
            next_insertion_order: 0,
        }
    }

    pub fn add_phase<P>(&mut self, group: PhaseGroup, phase: P)
    where
        P: Phase<S, C, R, L> + 'static,
    {
        self.phases.push(PhaseEntry {
            group,
            insertion_order: self.next_insertion_order,
            phase: Box::new(phase),
        });
        self.next_insertion_order += 1;
    }

    pub fn phase_order(&self) -> Vec<&'static str> {
        <Self as Scheduler<S, C, R, L>>::phase_order(self)
    }

    pub fn group_members(&self, group: PhaseGroup) -> Vec<&'static str> {
        <Self as Scheduler<S, C, R, L>>::group_members(self, group)
    }
}

impl<S, C, R, L> Default for FixedScheduler<S, C, R, L>
where
    S: SimulationState,
    C: Command,
    R: Rng,
    L: ReplayLog<C, S::Snapshot>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<S, C, R, L> Scheduler<S, C, R, L> for FixedScheduler<S, C, R, L>
where
    S: SimulationState,
    C: Command,
    R: Rng,
    L: ReplayLog<C, S::Snapshot>,
{
    fn visit_phases(
        &mut self,
        visitor: &mut dyn FnMut(
            PhaseDescriptor,
            &mut dyn Phase<S, C, R, L>,
        ) -> Result<(), EngineError>,
    ) -> Result<(), EngineError> {
        let mut ordinal = 0usize;

        for group in PhaseGroup::ALL {
            for entry in self.phases.iter_mut().filter(|entry| entry.group == group) {
                let descriptor = PhaseDescriptor {
                    ordinal,
                    group,
                    name: entry.phase.name(),
                };
                visitor(descriptor, entry.phase.as_mut())?;
                ordinal += 1;
            }
        }

        Ok(())
    }

    fn phase_plan(&self) -> Vec<PhaseDescriptor> {
        let mut plan = Vec::with_capacity(self.phases.len());
        let mut ordinal = 0usize;

        for group in PhaseGroup::ALL {
            let mut members = self
                .phases
                .iter()
                .filter(|entry| entry.group == group)
                .collect::<Vec<_>>();
            members.sort_by_key(|entry| entry.insertion_order);

            for entry in members {
                plan.push(PhaseDescriptor {
                    ordinal,
                    group,
                    name: entry.phase.name(),
                });
                ordinal += 1;
            }
        }

        plan
    }
}
