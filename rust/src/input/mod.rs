use std::fmt::Debug;

use crate::core::Tick;

pub trait Command: Clone + Debug + Eq {}

impl<T> Command for T where T: Clone + Debug + Eq {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputFrame<C: Command> {
    pub tick: Tick,
    pub command: C,
}

impl<C: Command> InputFrame<C> {
    pub const fn new(tick: Tick, command: C) -> Self {
        Self { tick, command }
    }
}
