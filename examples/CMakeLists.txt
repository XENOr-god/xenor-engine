add_executable(
  xenor_engine_resource_pipeline_example
  resource_pipeline_example.cpp)

target_link_libraries(
  xenor_engine_resource_pipeline_example
  PRIVATE xenor::engine)

xenor_engine_configure_warnings(xenor_engine_resource_pipeline_example)
