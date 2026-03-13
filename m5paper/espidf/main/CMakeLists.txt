idf_component_register(SRCS "main.c" INCLUDE_DIRS "." REQUIRES m5paper_backend pthread freertos)

if(NOT DEFINED ENV{TERN_RUST_LIB_PATH})
    message(FATAL_ERROR "TERN_RUST_LIB_PATH is not set")
endif()

add_prebuilt_library(tern_rust "$ENV{TERN_RUST_LIB_PATH}"
    PRIV_REQUIRES pthread m5paper_backend arduino-esp32)

target_link_libraries(${COMPONENT_LIB} PRIVATE tern_rust)
