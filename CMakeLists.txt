cmake_minimum_required(VERSION 3.24)

project(
  xenor-engine
  VERSION 0.1.0
  DESCRIPTION "Deterministic fixed-timestep simulation engine"
  LANGUAGES CXX)

if(CMAKE_SOURCE_DIR STREQUAL CMAKE_BINARY_DIR)
  message(FATAL_ERROR "In-source builds are not supported. Configure from a separate build directory.")
endif()

include(GNUInstallDirs)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(PROJECT_IS_TOP_LEVEL)
  set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
endif()

option(XENOR_ENGINE_BUILD_EXAMPLES "Build example programs." ON)
option(XENOR_ENGINE_BUILD_TESTS "Build unit tests." ON)
option(XENOR_ENGINE_BUILD_BENCHMARKS "Build benchmark targets." ON)

include(cmake/CompilerWarnings.cmake)

add_library(
  xenor_engine
  src/simulation_clock.cpp
  src/simulation_config.cpp)
add_library(xenor::engine ALIAS xenor_engine)

target_compile_features(xenor_engine PUBLIC cxx_std_20)
target_include_directories(
  xenor_engine
  PUBLIC
    $<BUILD_INTERFACE:${PROJECT_SOURCE_DIR}/include>
    $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>)

xenor_engine_configure_warnings(xenor_engine)

target_sources(
  xenor_engine
  PUBLIC
    FILE_SET public_headers
    TYPE HEADERS
    BASE_DIRS ${PROJECT_SOURCE_DIR}/include
    FILES
      include/xenor/simulation_clock.hpp
      include/xenor/simulation_config.hpp
      include/xenor/deterministic_rng.hpp
      include/xenor/input_sequence.hpp
      include/xenor/simulation_engine.hpp
      include/xenor/simulation_snapshot.hpp
      include/xenor/simulation_state.hpp
      include/xenor/step_context.hpp
      include/xenor/xenor.hpp)

install(
  TARGETS xenor_engine
  EXPORT xenor-engine-targets
  FILE_SET public_headers DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})

install(
  EXPORT xenor-engine-targets
  NAMESPACE xenor::
  DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/xenor-engine)

if(XENOR_ENGINE_BUILD_EXAMPLES)
  add_subdirectory(examples)
endif()

if(XENOR_ENGINE_BUILD_TESTS)
  include(CTest)
  if(BUILD_TESTING)
    add_subdirectory(tests)
  endif()
endif()

if(XENOR_ENGINE_BUILD_BENCHMARKS)
  add_subdirectory(benchmarks)
endif()
