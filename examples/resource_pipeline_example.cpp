#include <algorithm>
#include <chrono>
#include <cstdint>
#include <cstdlib>
#include <iostream>

#include "xenor/xenor.hpp"

namespace {

struct ResourceTickInput {
  std::uint64_t inbound_ore{0};
  std::uint64_t outbound_target{0};

  bool operator==(const ResourceTickInput&) const = default;
};

struct ResourcePipelineState final : xenor::SimulationState {
  std::uint64_t ore{0};
  std::uint64_t ingots{0};
  std::uint64_t finished_units{0};
  std::uint64_t backlog{0};
  std::uint64_t random_checksum{0};
};

struct ResourcePipelinePayload {
  std::uint64_t ore{0};
  std::uint64_t ingots{0};
  std::uint64_t finished_units{0};
  std::uint64_t backlog{0};
  std::uint64_t random_checksum{0};

  bool operator==(const ResourcePipelinePayload&) const = default;
};

bool identical(const ResourcePipelineState& left, const ResourcePipelineState& right) {
  return left.ore == right.ore && left.ingots == right.ingots &&
         left.finished_units == right.finished_units &&
         left.backlog == right.backlog &&
         left.random_checksum == right.random_checksum &&
         left.last_completed_tick() == right.last_completed_tick();
}

bool identical(const xenor::SimulationSnapshot<ResourcePipelineState>& left,
               const xenor::SimulationSnapshot<ResourcePipelineState>& right) {
  return left.tick == right.tick && left.elapsed == right.elapsed &&
         left.seed == right.seed && identical(left.state, right.state);
}

struct ResourcePipelineSnapshotAdapter {
  using payload_type = ResourcePipelinePayload;

  payload_type capture(const ResourcePipelineState& state) {
    return payload_type{
        .ore = state.ore,
        .ingots = state.ingots,
        .finished_units = state.finished_units,
        .backlog = state.backlog,
        .random_checksum = state.random_checksum,
    };
  }

  ResourcePipelineState restore(const payload_type& payload) {
    ResourcePipelineState state;
    state.ore = payload.ore;
    state.ingots = payload.ingots;
    state.finished_units = payload.finished_units;
    state.backlog = payload.backlog;
    state.random_checksum = payload.random_checksum;
    return state;
  }
};

auto make_inputs() {
  return xenor::InputSequence<ResourceTickInput>{
      {{3, 1}, {1, 2}, {4, 2}, {2, 3}, {5, 2}, {1, 1}, {3, 3}, {2, 1}}};
}

auto make_engine(xenor::SimulationConfig::seed_type seed) {
  using namespace std::chrono_literals;

  auto engine =
      xenor::SimulationEngine<ResourcePipelineState, ResourceTickInput>{
          xenor::SimulationConfig{50ms, seed}};

  engine.add_system(
      xenor::SystemPhase::PreUpdate,
      "intake",
      [](ResourcePipelineState& state,
         const xenor::InputStepContext<ResourceTickInput>& context) {
        state.ore += context.input().inbound_ore;
      });

  engine.add_system(
      xenor::SystemPhase::PreUpdate,
      "yield_adjustment",
      [](ResourcePipelineState& state,
         const xenor::InputStepContext<ResourceTickInput>& context) {
        const auto random_value = context.rng().next_u64();
        state.random_checksum ^= random_value + context.step_seed;
        state.ore += random_value % 2ULL;
      });

  engine.add_system(
      xenor::SystemPhase::Update,
      "smelting",
      [](ResourcePipelineState& state,
         const xenor::InputStepContext<ResourceTickInput>&) {
        const auto ingot_batches = state.ore / 2;
        state.ore -= ingot_batches * 2;
        state.ingots += ingot_batches;
      });

  engine.add_system(
      xenor::SystemPhase::PostUpdate,
      "dispatch",
      [](ResourcePipelineState& state,
         const xenor::InputStepContext<ResourceTickInput>& context) {
        const auto finished_batches =
            std::min(state.ingots, context.input().outbound_target);
        state.ingots -= finished_batches;
        state.finished_units += finished_batches;
        state.backlog += context.input().outbound_target - finished_batches;
      });

  return engine;
}

template <typename Input>
std::size_t count_events(const xenor::ReplayTrace<Input>& trace,
                         xenor::ReplayEventKind kind) {
  return static_cast<std::size_t>(std::count_if(
      trace.events.begin(), trace.events.end(), [kind](const auto& event) {
        return event.kind == kind;
      }));
}

}  // namespace

int main() {
  constexpr xenor::SimulationConfig::seed_type seed = 41;
  constexpr std::size_t checkpoint_ticks = 5;

  const auto inputs = make_inputs();
  const auto initial_inputs = inputs.slice(0, checkpoint_ticks);
  const auto remaining_inputs = inputs.slice(checkpoint_ticks);

  auto uninterrupted = make_engine(seed);
  auto restored = make_engine(seed);
  auto repeated = make_engine(seed);
  auto boundary_restored = make_engine(seed);
  ResourcePipelineSnapshotAdapter snapshot_adapter;

  uninterrupted.enable_replay_capture();
  restored.enable_replay_capture();
  repeated.enable_replay_capture();

  uninterrupted.run_for_sequence(inputs);
  const auto uninterrupted_final = uninterrupted.capture_snapshot();

  restored.run_for_sequence(initial_inputs);
  const auto checkpoint = restored.capture_snapshot();

  restored.run_for_sequence(remaining_inputs);
  const auto first_continuation = restored.capture_snapshot();

  restored.restore_snapshot(checkpoint);
  restored.run_for_sequence(remaining_inputs);
  const auto replayed_continuation = restored.capture_snapshot();

  repeated.run_for_sequence(inputs);
  const auto repeated_final = repeated.capture_snapshot();

  const auto serialized_boundary = uninterrupted.capture_snapshot_boundary(snapshot_adapter);
  boundary_restored.restore_snapshot_boundary(serialized_boundary, snapshot_adapter);
  const auto boundary_restored_snapshot = boundary_restored.capture_snapshot();

  if (!identical(uninterrupted_final, repeated_final) ||
      !identical(first_continuation, replayed_continuation) ||
      !identical(uninterrupted_final, replayed_continuation) ||
      !identical(uninterrupted_final, boundary_restored_snapshot) ||
      !(uninterrupted.replay_trace() == repeated.replay_trace()) ||
      count_events(restored.replay_trace(), xenor::ReplayEventKind::SnapshotRestored) != 1) {
    std::cerr << "deterministic input replay check failed\n";
    return EXIT_FAILURE;
  }

  const auto checkpoint_elapsed =
      std::chrono::duration_cast<std::chrono::milliseconds>(checkpoint.elapsed);
  const auto elapsed =
      std::chrono::duration_cast<std::chrono::milliseconds>(uninterrupted_final.elapsed);

  std::cout << "seed: " << seed << '\n';
  std::cout << "checkpoint_tick: " << checkpoint.tick << '\n';
  std::cout << "checkpoint_elapsed_ms: " << checkpoint_elapsed.count() << '\n';
  std::cout << "tick: " << uninterrupted_final.tick << '\n';
  std::cout << "elapsed_ms: " << elapsed.count() << '\n';
  std::cout << "ore: " << uninterrupted_final.state.ore << '\n';
  std::cout << "ingots: " << uninterrupted_final.state.ingots << '\n';
  std::cout << "finished_units: " << uninterrupted_final.state.finished_units << '\n';
  std::cout << "backlog: " << uninterrupted_final.state.backlog << '\n';
  std::cout << "random_checksum: " << uninterrupted_final.state.random_checksum << '\n';
  std::cout << "boundary_tick: " << serialized_boundary.metadata.tick << '\n';
  std::cout << "boundary_state_last_completed_tick: "
            << serialized_boundary.metadata.state_last_completed_tick << '\n';
  std::cout << "boundary_payload_finished_units: "
            << serialized_boundary.state_payload.finished_units << '\n';
  std::cout << "trace_events: " << uninterrupted.replay_trace().events.size() << '\n';
  std::cout << "trace_system_events: "
            << count_events(uninterrupted.replay_trace(), xenor::ReplayEventKind::SystemExecuted)
            << '\n';
  std::cout << "trace_restore_events: "
            << count_events(restored.replay_trace(), xenor::ReplayEventKind::SnapshotRestored)
            << '\n';

  return EXIT_SUCCESS;
}
