#pragma once

#include "xenor/simulation_clock.hpp"

namespace xenor {

template <typename State>
struct SimulationSnapshot {
  SimulationClock::tick_type tick{0};
  SimulationConfig::duration_type elapsed{};
  State state{};
};

}  // namespace xenor
