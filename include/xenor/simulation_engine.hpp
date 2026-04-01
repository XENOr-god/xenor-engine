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
#include "xenor/replay_trace.hpp"
#include "xenor/snapshot_boundary.hpp"
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
  using replay_trace_type = ReplayTrace<Input>;
  using system_type = std::function<void(State&, const step_context_type&)>;

  static_assert(std::copyable<State>,
                "SimulationEngine state must be copyable for snapshot capture and restore.");
  static_assert(std::same_as<Input, NoInput> || std::copy_constructible<Input>,
                "SimulationEngine input must be copy constructible for replay trace capture.");

  explicit SimulationEngine(SimulationConfig config, State initial_state = {})
      : config_(std::move(config)), clock_(config_), state_(std::move(initial_state)) {
    state_.set_last_completed_tick(clock_.current_tick());
    replay_trace_.seed = config_.seed();
  }

  [[nodiscard]] const SimulationConfig& config() const noexcept { return config_; }
  [[nodiscard]] const SimulationClock& clock() const noexcept { return clock_; }
  [[nodiscard]] const State& state() const noexcept { return state_; }
  [[nodiscard]] State& state() noexcept { return state_; }
  [[nodiscard]] seed_type seed() const noexcept { return config_.seed(); }
  [[nodiscard]] bool replay_capture_enabled() const noexcept { return replay_capture_enabled_; }
  [[nodiscard]] const replay_trace_type& replay_trace() const noexcept { return replay_trace_; }

  void enable_replay_capture() {
    replay_capture_enabled_ = true;
    clear_replay_trace();
  }

  void disable_replay_capture() noexcept { replay_capture_enabled_ = false; }

  void clear_replay_trace() {
    replay_trace_.seed = config_.seed();
    replay_trace_.clear();
  }

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

  template <typename Adapter>
    requires SnapshotStateAdapter<Adapter, State>
  [[nodiscard]] auto capture_snapshot_boundary(Adapter& adapter) const
      -> SnapshotBoundary<typename Adapter::payload_type> {
    return make_snapshot_boundary(capture_snapshot(), adapter);
  }

  void restore_snapshot(const SimulationSnapshot<State>& snapshot) {
    validate_snapshot(snapshot);
    clock_.restore(snapshot.tick);
    state_ = snapshot.state;
    record_replay_event(replay_trace_type::event_type::snapshot_restored(
        snapshot.tick, snapshot.elapsed));
  }

  template <typename Payload, typename Adapter>
    requires SnapshotStateAdapter<Adapter, State> &&
             std::same_as<typename Adapter::payload_type, Payload>
  void restore_snapshot_boundary(const SnapshotBoundary<Payload>& boundary,
                                 Adapter& adapter) {
    validate_snapshot_boundary_metadata(boundary.metadata, config_);
    const auto restored_snapshot = restore_snapshot_from_boundary<State>(boundary, adapter);

    if (restored_snapshot.tick != boundary.metadata.tick ||
        restored_snapshot.elapsed != boundary.metadata.elapsed ||
        restored_snapshot.seed != boundary.metadata.seed ||
        restored_snapshot.state.last_completed_tick() != boundary.metadata.state_last_completed_tick) {
      throw_snapshot_boundary_error(
          SnapshotBoundaryErrorCode::InvalidStateMetadata,
          "snapshot boundary restore produced a snapshot that does not match "
          "the validated boundary metadata");
    }

    clock_.restore(restored_snapshot.tick);
    state_ = restored_snapshot.state;
    record_replay_event(replay_trace_type::event_type::snapshot_restored(
        restored_snapshot.tick, restored_snapshot.elapsed));
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

    record_replay_event(
        replay_trace_type::event_type::tick_started(next_tick, context.elapsed, step_seed));

    if constexpr (!std::same_as<Input, NoInput>) {
      record_replay_input(context.tick, context.elapsed, input);
    }

    for (const auto phase : ordered_system_phases()) {
      for (const auto& system : systems_) {
        if (system.phase != phase) {
          continue;
        }

        record_replay_event(replay_trace_type::event_type::system_executed(
            context.tick, context.elapsed, system.phase, system.name));
        system.callback(state_, context);
      }
    }

    clock_.advance();
    state_.set_last_completed_tick(clock_.current_tick());
    record_replay_event(replay_trace_type::event_type::tick_completed(
        clock_.current_tick(), clock_.elapsed_duration()));
  }

  [[nodiscard]] static seed_type derive_step_seed(seed_type seed, tick_type tick) noexcept {
    return DeterministicRng::mix(
        seed + 0x9e3779b97f4a7c15ULL * static_cast<seed_type>(tick));
  }

  void record_replay_event(typename replay_trace_type::event_type event) {
    if (!replay_capture_enabled_) {
      return;
    }

    replay_trace_.events.push_back(std::move(event));
  }

  void record_replay_input(tick_type tick, duration_type elapsed, const Input* input)
      requires (!std::same_as<Input, NoInput>) {
    if (!replay_capture_enabled_ || input == nullptr) {
      return;
    }

    replay_trace_.events.push_back(
        replay_trace_type::event_type::input_applied(tick, elapsed, *input));
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
  replay_trace_type replay_trace_{};
  bool replay_capture_enabled_{false};
  std::vector<RegisteredSystem> systems_;
};

}  // namespace xenor
