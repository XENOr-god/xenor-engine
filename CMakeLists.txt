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

add_library(xenor_engine INTERFACE)
add_library(xenor::engine ALIAS xenor_engine)

target_compile_features(xenor_engine INTERFACE cxx_std_20)
target_include_directories(
  xenor_engine
  INTERFACE
    $<BUILD_INTERFACE:${PROJECT_SOURCE_DIR}/include>
    $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>)

install(
  TARGETS xenor_engine
  EXPORT xenor-engine-targets)

install(
  EXPORT xenor-engine-targets
  NAMESPACE xenor::
  DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/xenor-engine)
