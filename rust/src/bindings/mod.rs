use crate::core::EngineError;
use crate::engine::{Engine, TickResult};
use crate::input::{Command, InputFrame};

pub struct EngineBinding<E> {
    engine: E,
}

impl<E> EngineBinding<E> {
    pub fn new(engine: E) -> Self {
        Self { engine }
    }

    pub fn into_inner(self) -> E {
        self.engine
    }
}

impl<E> EngineBinding<E> {
    pub fn tick<C>(&mut self, frame: InputFrame<C>) -> Result<TickResult<E::State>, EngineError>
    where
        E: Engine<C>,
        C: Command,
    {
        self.engine.tick(frame)
    }

    pub fn engine(&self) -> &E {
        &self.engine
    }
}
