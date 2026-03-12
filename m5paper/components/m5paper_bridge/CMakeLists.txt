idf_component_register(
    SRCS
        "m5paper_bridge.cpp"
        "../../../../M5EPD/src/M5EPD_Driver.cpp"
        "../../../../M5EPD/src/utility/BM8563.cpp"
        "../../../../M5EPD/src/utility/GT911.cpp"
    INCLUDE_DIRS
        "."
        "../../cshim"
        "../../../../M5EPD/src"
        "../../../../M5EPD/src/utility"
    REQUIRES
        arduino-esp32
        driver
)

target_compile_features(${COMPONENT_LIB} PRIVATE cxx_std_17)
target_compile_options(${COMPONENT_LIB} PRIVATE -Wno-unused-parameter)
