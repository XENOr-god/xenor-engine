#pragma once

#include <concepts>
#include <cstdint>
#include <exception>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>

#include "xenor/simulation_clock.hpp"
#include "xenor/simulation_config.hpp"
#include "xenor/simulation_snapshot.hpp"
#include "xenor/simulation_state.hpp"

namespace xenor {

using snapshot_boundary_engine_version_type = std::uint32_t;
using snapshot_boundary_payload_version_type = std::uint32_t;

inline constexpr snapshot_boundary_engine_version_type
    current_snapshot_boundary_engine_version = 1;

enum class SnapshotBoundaryErrorCode : std::uint8_t {
  IncompatibleEngineVersion,
  InvalidStateMetadata,
  IncompatibleElapsedDuration,
  IncompatibleSeed,
  UnsupportedPayloadVersion,
  AdapterContractViolation,
  AdapterRestoreFailure,
};

[[nodiscard]] constexpr std::string_view
to_string(SnapshotBoundaryErrorCode code) noexcept {
  switch (code) {
    case SnapshotBoundaryErrorCode::IncompatibleEngineVersion:
      return "IncompatibleEngineVersion";
    case SnapshotBoundaryErrorCode::InvalidStateMetadata:
      return "InvalidStateMetadata";
    case SnapshotBoundaryErrorCode::IncompatibleElapsedDuration:
      return "IncompatibleElapsedDuration";
    case SnapshotBoundaryErrorCode::IncompatibleSeed:
      return "IncompatibleSeed";
    case SnapshotBoundaryErrorCode::UnsupportedPayloadVersion:
      return "UnsupportedPayloadVersion";
    case SnapshotBoundaryErrorCode::AdapterContractViolation:
      return "AdapterContractViolation";
    case SnapshotBoundaryErrorCode::AdapterRestoreFailure:
      return "AdapterRestoreFailure";
  }

  return "Unknown";
}

class SnapshotBoundaryError : public std::runtime_error {
public:
  SnapshotBoundaryError(SnapshotBoundaryErrorCode code, std::string message)
      : std::runtime_error(std::move(message)), code_(code) {}

  [[nodiscard]] SnapshotBoundaryErrorCode code() const noexcept { return code_; }

private:
  SnapshotBoundaryErrorCode code_;
};

[[noreturn]] inline void throw_snapshot_boundary_error(
    SnapshotBoundaryErrorCode code,
    std::string message) {
  throw SnapshotBoundaryError{code, std::move(message)};
}

struct SnapshotBoundaryMetadata {
  snapshot_boundary_engine_version_type engine_version{
      current_snapshot_boundary_engine_version};
  SimulationClock::tick_type tick{0};
  SimulationConfig::duration_type elapsed{};
  SimulationConfig::seed_type seed{0};
  SimulationClock::tick_type state_last_completed_tick{0};

  bool operator==(const SnapshotBoundaryMetadata&) const = default;
};

template <typename Payload>
struct SnapshotBoundary {
  SnapshotBoundaryMetadata metadata{};
  snapshot_boundary_payload_version_type payload_version{0};
  Payload state_payload{};

  bool operator==(const SnapshotBoundary&) const = default;
};

template <typename Adapter, typename State>
concept SnapshotStateAdapter =
    std::derived_from<State, SimulationState> &&
    requires(Adapter& adapter,
             const State& state,
             const typename Adapter::payload_type& payload,
             snapshot_boundary_payload_version_type payload_version) {
      typename Adapter::payload_type;
      { adapter.payload_version() } ->
          std::convertible_to<snapshot_boundary_payload_version_type>;
      { adapter.supports_payload_version(payload_version) } -> std::convertible_to<bool>;
      { adapter.capture(state) } -> std::same_as<typename Adapter::payload_type>;
      { adapter.restore(payload, payload_version) } -> std::same_as<State>;
    };

inline void validate_snapshot_boundary_engine_version(
    snapshot_boundary_engine_version_type engine_version) {
  if (engine_version != current_snapshot_boundary_engine_version) {
    throw_snapshot_boundary_error(
        SnapshotBoundaryErrorCode::IncompatibleEngineVersion,
        "snapshot boundary engine version " + std::to_string(engine_version) +
            " is not supported; expected " +
            std::to_string(current_snapshot_boundary_engine_version));
  }
}

template <typename Adapter>
void validate_snapshot_boundary_payload_version(
    snapshot_boundary_payload_version_type payload_version,
    Adapter& adapter) {
  try {
    if (!adapter.supports_payload_version(payload_version)) {
      throw_snapshot_boundary_error(
          SnapshotBoundaryErrorCode::UnsupportedPayloadVersion,
          "snapshot boundary payload version " + std::to_string(payload_version) +
              " is not supported by the adapter");
    }
  } catch (const SnapshotBoundaryError&) {
    throw;
  } catch (const std::exception& exception) {
    throw_snapshot_boundary_error(
        SnapshotBoundaryErrorCode::AdapterContractViolation,
        "snapshot boundary adapter payload-version support check failed for "
        "version " + std::to_string(payload_version) + ": " + exception.what());
  } catch (...) {
    throw_snapshot_boundary_error(
        SnapshotBoundaryErrorCode::AdapterContractViolation,
        "snapshot boundary adapter payload-version support check failed for "
        "version " + std::to_string(payload_version) +
            ": adapter threw a non-standard exception");
  }
}

inline void validate_snapshot_boundary_metadata(
    const SnapshotBoundaryMetadata& metadata) {
  validate_snapshot_boundary_engine_version(metadata.engine_version);

  if (metadata.state_last_completed_tick != metadata.tick) {
    throw_snapshot_boundary_error(
        SnapshotBoundaryErrorCode::InvalidStateMetadata,
        "snapshot boundary state metadata is invalid: state_last_completed_tick " +
            std::to_string(metadata.state_last_completed_tick) +
            " does not match tick " + std::to_string(metadata.tick));
  }
}

inline void validate_snapshot_boundary_metadata(
    const SnapshotBoundaryMetadata& metadata,
    const SimulationConfig& config) {
  validate_snapshot_boundary_metadata(metadata);

  SimulationClock clock{config};
  if (metadata.elapsed != clock.elapsed_duration_at(metadata.tick)) {
    throw_snapshot_boundary_error(
        SnapshotBoundaryErrorCode::IncompatibleElapsedDuration,
        "snapshot boundary elapsed duration " +
            std::to_string(metadata.elapsed.count()) +
            " does not match the engine configuration for tick " +
            std::to_string(metadata.tick) + "; expected " +
            std::to_string(clock.elapsed_duration_at(metadata.tick).count()));
  }

  if (metadata.seed != config.seed()) {
    throw_snapshot_boundary_error(
        SnapshotBoundaryErrorCode::IncompatibleSeed,
        "snapshot boundary seed " + std::to_string(metadata.seed) +
            " does not match the engine configuration seed " +
            std::to_string(config.seed()));
  }
}

template <typename State>
  requires std::derived_from<State, SimulationState>
[[nodiscard]] SnapshotBoundaryMetadata make_snapshot_boundary_metadata(
    const SimulationSnapshot<State>& snapshot) {
  return SnapshotBoundaryMetadata{
      .engine_version = current_snapshot_boundary_engine_version,
      .tick = snapshot.tick,
      .elapsed = snapshot.elapsed,
      .seed = snapshot.seed,
      .state_last_completed_tick = snapshot.state.last_completed_tick(),
  };
}

template <typename State, typename Adapter>
  requires SnapshotStateAdapter<Adapter, State>
[[nodiscard]] auto make_snapshot_boundary(const SimulationSnapshot<State>& snapshot,
                                          Adapter& adapter)
    -> SnapshotBoundary<typename Adapter::payload_type> {
  const auto metadata = make_snapshot_boundary_metadata(snapshot);
  validate_snapshot_boundary_metadata(metadata);

  const auto payload_version =
      static_cast<snapshot_boundary_payload_version_type>(adapter.payload_version());
  try {
    if (!adapter.supports_payload_version(payload_version)) {
      throw_snapshot_boundary_error(
          SnapshotBoundaryErrorCode::AdapterContractViolation,
          "snapshot boundary adapter declared payload version " +
              std::to_string(payload_version) +
              " but does not report support for that version");
    }
  } catch (const SnapshotBoundaryError&) {
    throw;
  } catch (const std::exception& exception) {
    throw_snapshot_boundary_error(
        SnapshotBoundaryErrorCode::AdapterContractViolation,
        "snapshot boundary adapter self-validation failed for payload version " +
            std::to_string(payload_version) + ": " + exception.what());
  } catch (...) {
    throw_snapshot_boundary_error(
        SnapshotBoundaryErrorCode::AdapterContractViolation,
        "snapshot boundary adapter self-validation failed for payload version " +
            std::to_string(payload_version) +
            ": adapter threw a non-standard exception");
  }

  return SnapshotBoundary<typename Adapter::payload_type>{
      .metadata = metadata,
      .payload_version = payload_version,
      .state_payload = adapter.capture(snapshot.state),
  };
}

template <typename State, typename Payload, typename Adapter>
  requires SnapshotStateAdapter<Adapter, State> &&
           std::same_as<typename Adapter::payload_type, Payload>
[[nodiscard]] SimulationSnapshot<State> restore_snapshot_from_boundary(
    const SnapshotBoundary<Payload>& boundary,
    Adapter& adapter) {
  validate_snapshot_boundary_metadata(boundary.metadata);
  validate_snapshot_boundary_payload_version(boundary.payload_version, adapter);

  State state{};
  try {
    state = adapter.restore(boundary.state_payload, boundary.payload_version);
  } catch (const SnapshotBoundaryError&) {
    throw;
  } catch (const std::exception& exception) {
    throw_snapshot_boundary_error(
        SnapshotBoundaryErrorCode::AdapterRestoreFailure,
        "snapshot boundary adapter restore failed for payload version " +
            std::to_string(boundary.payload_version) + ": " + exception.what());
  } catch (...) {
    throw_snapshot_boundary_error(
        SnapshotBoundaryErrorCode::AdapterRestoreFailure,
        "snapshot boundary adapter restore failed for payload version " +
            std::to_string(boundary.payload_version) +
            ": adapter threw a non-standard exception");
  }

  state.set_last_completed_tick(boundary.metadata.state_last_completed_tick);
  if (state.last_completed_tick() != boundary.metadata.state_last_completed_tick) {
    throw_snapshot_boundary_error(
        SnapshotBoundaryErrorCode::InvalidStateMetadata,
        "snapshot boundary restored state metadata does not match the "
        "authoritative boundary metadata");
  }

  return SimulationSnapshot<State>{
      .tick = boundary.metadata.tick,
      .elapsed = boundary.metadata.elapsed,
      .seed = boundary.metadata.seed,
      .state = std::move(state),
  };
}

}  // namespace xenor
