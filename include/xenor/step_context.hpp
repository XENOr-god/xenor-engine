#pragma once

#include "xenor/simulation_clock.hpp"

namespace xenor {

struct StepContext {
  SimulationClock::tick_type tick{0};
  SimulationConfig::duration_type tick_duration{};
  SimulationConfig::duration_type elapsed{};
};

}  // namespace xenor
