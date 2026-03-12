idf_component_register(
    SRCS
        "m5paper_backend.cpp"
        "../../../../M5EPD/src/M5EPD_Driver.cpp"
        "../../../../M5EPD/src/utility/GT911.cpp"
        "../../../../M5EPD/src/utility/BM8563.cpp"
    INCLUDE_DIRS
        "."
        "../../../../M5EPD/src"
        "../../../../M5EPD/src/utility"
    REQUIRES
        arduino-esp32
        driver
        esp_timer
        esp_netif
        freertos
)

target_compile_features(${COMPONENT_LIB} PRIVATE cxx_std_17)
target_compile_options(${COMPONENT_LIB} PRIVATE -Wno-unused-parameter)
target_compile_definitions(${COMPONENT_LIB} PRIVATE BOARD_HAS_PSRAM)
