use crate::core::{Seed, fork_seed, mix64};

pub trait Rng: Clone {
    fn from_seed(seed: Seed) -> Self
    where
        Self: Sized;

    fn seed(&self) -> Seed;

    fn next_u64(&mut self) -> u64;

    fn fork(&self, stream: &'static str) -> Self
    where
        Self: Sized,
    {
        Self::from_seed(fork_seed(self.seed(), stream))
    }

    fn next_u32(&mut self) -> u32 {
        self.next_u64() as u32
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitMix64 {
    origin: Seed,
    state: u64,
}

impl Rng for SplitMix64 {
    fn from_seed(seed: Seed) -> Self {
        Self {
            origin: seed,
            state: seed,
        }
    }

    fn seed(&self) -> Seed {
        self.origin
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        mix64(self.state)
    }
}
