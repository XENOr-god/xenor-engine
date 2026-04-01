#include "xenor/simulation_config.hpp"

#include <stdexcept>

namespace xenor {

SimulationConfig::SimulationConfig(duration_type tick_duration) : tick_duration_(tick_duration) {
  if (tick_duration_ <= duration_type::zero()) {
    throw std::invalid_argument("tick duration must be greater than zero");
  }
}

SimulationConfig::duration_type SimulationConfig::tick_duration() const noexcept {
  return tick_duration_;
}

std::int64_t SimulationConfig::tick_duration_count() const noexcept {
  return tick_duration_.count();
}

}  // namespace xenor
