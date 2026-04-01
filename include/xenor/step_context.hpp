#pragma once

#include <concepts>
#include <stdexcept>

#include "xenor/deterministic_rng.hpp"
#include "xenor/simulation_clock.hpp"

namespace xenor {

struct NoInput final {};

template <typename Input = NoInput>
class InputStepContext {
public:
  using seed_type = SimulationConfig::seed_type;
  using input_type = Input;

  SimulationClock::tick_type tick{0};
  SimulationConfig::duration_type tick_duration{};
  SimulationConfig::duration_type elapsed{};
  seed_type seed{0};
  seed_type step_seed{0};

  constexpr InputStepContext() noexcept = default;

  constexpr InputStepContext(SimulationClock::tick_type tick_value,
                             SimulationConfig::duration_type tick_duration_value,
                             SimulationConfig::duration_type elapsed_value,
                             seed_type seed_value,
                             seed_type step_seed_value,
                             const Input* input,
                             DeterministicRng* rng) noexcept
      : tick(tick_value),
        tick_duration(tick_duration_value),
        elapsed(elapsed_value),
        seed(seed_value),
        step_seed(step_seed_value),
        input_(input),
        rng_(rng) {}

  [[nodiscard]] bool has_input() const noexcept requires (!std::same_as<Input, NoInput>) {
    return input_ != nullptr;
  }

  [[nodiscard]] const Input& input() const requires (!std::same_as<Input, NoInput>) {
    if (input_ == nullptr) {
      throw std::logic_error("step context does not have an input value");
    }

    return *input_;
  }

  [[nodiscard]] bool has_rng() const noexcept { return rng_ != nullptr; }

  [[nodiscard]] DeterministicRng& rng() const {
    if (rng_ == nullptr) {
      throw std::logic_error("step context does not have a deterministic random source");
    }

    return *rng_;
  }

private:
  const Input* input_{nullptr};
  DeterministicRng* rng_{nullptr};
};

using StepContext = InputStepContext<NoInput>;

}  // namespace xenor
