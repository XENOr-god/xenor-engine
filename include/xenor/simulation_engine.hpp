#pragma once

#include <concepts>
#include <cstddef>
#include <functional>
#include <limits>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "xenor/input_sequence.hpp"
#include "xenor/simulation_clock.hpp"
#include "xenor/simulation_config.hpp"
#include "xenor/simulation_snapshot.hpp"
#include "xenor/simulation_state.hpp"
#include "xenor/step_context.hpp"
#include "xenor/system_phase.hpp"

namespace xenor {

template <typename State>
concept EngineState = std::derived_from<State, SimulationState> && std::copyable<State>;

template <EngineState State, typename Input = NoInput>
class SimulationEngine {
public:
  using state_type = State;
  using input_type = Input;
  using tick_type = SimulationClock::tick_type;
  using duration_type = SimulationConfig::duration_type;
  using seed_type = SimulationConfig::seed_type;
  using phase_type = SystemPhase;
  using step_context_type = InputStepContext<Input>;
  using system_type = std::function<void(State&, const step_context_type&)>;

  static_assert(std::copyable<State>,
                "SimulationEngine state must be copyable for snapshot capture and restore.");

  explicit SimulationEngine(SimulationConfig config, State initial_state = {})
      : config_(std::move(config)), clock_(config_), state_(std::move(initial_state)) {
    state_.set_last_completed_tick(clock_.current_tick());
  }

  [[nodiscard]] const SimulationConfig& config() const noexcept { return config_; }
  [[nodiscard]] const SimulationClock& clock() const noexcept { return clock_; }
  [[nodiscard]] const State& state() const noexcept { return state_; }
  [[nodiscard]] State& state() noexcept { return state_; }
  [[nodiscard]] seed_type seed() const noexcept { return config_.seed(); }

  std::size_t add_system(std::string name, system_type system) {
    return add_system(phase_type::Update, std::move(name), std::move(system));
  }

  std::size_t add_system(phase_type phase, std::string name, system_type system) {
    if (name.empty()) {
      throw std::invalid_argument("system name must not be empty");
    }

    if (!system) {
      throw std::invalid_argument("system callback must not be empty");
    }

    systems_.push_back(RegisteredSystem{
        .phase = phase,
        .name = std::move(name),
        .callback = std::move(system),
    });
    return systems_.size() - 1;
  }

  [[nodiscard]] std::size_t system_count() const noexcept { return systems_.size(); }

  [[nodiscard]] std::vector<std::string> system_names() const {
    std::vector<std::string> names;
    names.reserve(systems_.size());

    for (const auto& system : systems_) {
      names.push_back(system.name);
    }

    return names;
  }

  void step() requires std::same_as<Input, NoInput> {
    step_impl(nullptr);
  }

  void step(const Input& input) requires (!std::same_as<Input, NoInput>) {
    step_impl(&input);
  }

  void run_for_ticks(tick_type ticks) requires std::same_as<Input, NoInput> {
    for (tick_type tick = 0; tick < ticks; ++tick) {
      step();
    }
  }

  void run_for_sequence(const InputSequence<Input>& inputs)
      requires (!std::same_as<Input, NoInput>) {
    for (const auto& input : inputs) {
      step(input);
    }
  }

  [[nodiscard]] SimulationSnapshot<State> capture_snapshot() const {
    return SimulationSnapshot<State>{
        .tick = clock_.current_tick(),
        .elapsed = clock_.elapsed_duration(),
        .seed = config_.seed(),
        .state = state_,
    };
  }

  [[nodiscard]] SimulationSnapshot<State> snapshot() const { return capture_snapshot(); }

  void restore_snapshot(const SimulationSnapshot<State>& snapshot) {
    validate_snapshot(snapshot);
    clock_.restore(snapshot.tick);
    state_ = snapshot.state;
  }

private:
  struct RegisteredSystem {
    phase_type phase{phase_type::Update};
    std::string name;
    system_type callback;
  };

  void step_impl(const Input* input) {
    const auto current_tick = clock_.current_tick();
    if (current_tick == std::numeric_limits<tick_type>::max()) {
      throw std::overflow_error("simulation tick overflow");
    }

    const auto next_tick = current_tick + 1;
    const auto step_seed = derive_step_seed(config_.seed(), next_tick);
    DeterministicRng step_rng{step_seed};
    const step_context_type context{
        next_tick,
        config_.tick_duration(),
        clock_.elapsed_duration_at(next_tick),
        config_.seed(),
        step_seed,
        input,
        &step_rng,
    };

    for (const auto phase : ordered_system_phases()) {
      for (const auto& system : systems_) {
        if (system.phase != phase) {
          continue;
        }

        system.callback(state_, context);
      }
    }

    clock_.advance();
    state_.set_last_completed_tick(clock_.current_tick());
  }

  [[nodiscard]] static seed_type derive_step_seed(seed_type seed, tick_type tick) noexcept {
    return DeterministicRng::mix(
        seed + 0x9e3779b97f4a7c15ULL * static_cast<seed_type>(tick));
  }

  void validate_snapshot(const SimulationSnapshot<State>& snapshot) const {
    if (snapshot.elapsed != clock_.elapsed_duration_at(snapshot.tick)) {
      throw std::invalid_argument(
          "snapshot elapsed duration does not match the engine configuration");
    }

    if (snapshot.seed != config_.seed()) {
      throw std::invalid_argument("snapshot seed does not match the engine configuration");
    }

    if (snapshot.state.last_completed_tick() != snapshot.tick) {
      throw std::invalid_argument(
          "snapshot state metadata does not match the captured tick");
    }
  }

  SimulationConfig config_;
  SimulationClock clock_;
  State state_;
  std::vector<RegisteredSystem> systems_;
};

}  // namespace xenor
