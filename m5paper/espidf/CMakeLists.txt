cmake_minimum_required(VERSION 3.16)

set(ENV{IDF_COMPONENT_MANAGER} "0")

set(EXTRA_COMPONENT_DIRS
    "${CMAKE_SOURCE_DIR}/../components"
    "${CMAKE_SOURCE_DIR}/../../third_party"
)

include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(tern_m5paper_rust_host)
