#include "xenor/simulation_clock.hpp"

#include <limits>
#include <stdexcept>

namespace xenor {

namespace {

SimulationClock::duration_type checked_duration_product(
    SimulationClock::duration_type tick_duration,
    SimulationClock::tick_type tick) {
  if (tick == 0) {
    return SimulationClock::duration_type::zero();
  }

  using rep = SimulationClock::duration_type::rep;
  const auto duration_count = tick_duration.count();
  const auto max_rep = std::numeric_limits<rep>::max();
  const auto max_tick =
      static_cast<SimulationClock::tick_type>(max_rep / duration_count);

  if (tick > max_tick) {
    throw std::overflow_error("simulated duration overflow");
  }

  return SimulationClock::duration_type{
      static_cast<rep>(duration_count * static_cast<rep>(tick))};
}

}  // namespace

SimulationClock::SimulationClock(SimulationConfig config)
    : tick_duration_(config.tick_duration()) {}

SimulationClock::tick_type SimulationClock::current_tick() const noexcept {
  return current_tick_;
}

SimulationClock::duration_type SimulationClock::tick_duration() const noexcept {
  return tick_duration_;
}

SimulationClock::duration_type SimulationClock::elapsed_duration() const {
  return elapsed_duration_at(current_tick_);
}

SimulationClock::duration_type SimulationClock::elapsed_duration_at(
    tick_type tick) const {
  return checked_duration_product(tick_duration_, tick);
}

void SimulationClock::advance() {
  advance_by(1);
}

void SimulationClock::advance_by(tick_type steps) {
  if (steps > (std::numeric_limits<tick_type>::max() - current_tick_)) {
    throw std::overflow_error("simulation tick overflow");
  }

  current_tick_ += steps;
}

void SimulationClock::restore(tick_type tick) {
  static_cast<void>(elapsed_duration_at(tick));
  current_tick_ = tick;
}

void SimulationClock::reset() noexcept {
  current_tick_ = 0;
}

}  // namespace xenor
