fn main() {
    embuild::espidf::sysenv::output();

    if std::env::var_os("CARGO_FEATURE_CSHIM").is_some() {
        println!("cargo:rerun-if-changed=components/m5paper_bridge/CMakeLists.txt");
        println!("cargo:rerun-if-changed=components/m5paper_bridge/idf_component.yml");
        println!("cargo:rerun-if-changed=components/m5paper_bridge/m5paper_bridge.cpp");
        println!("cargo:rerun-if-changed=cshim/m5paper_bridge.h");
        println!("cargo:rerun-if-changed=../../M5EPD/src/M5EPD_Driver.cpp");
        println!("cargo:rerun-if-changed=../../M5EPD/src/M5EPD_Driver.h");
        println!("cargo:rerun-if-changed=../../M5EPD/src/utility/BM8563.cpp");
        println!("cargo:rerun-if-changed=../../M5EPD/src/utility/BM8563.h");
        println!("cargo:rerun-if-changed=../../M5EPD/src/utility/GT911.cpp");
        println!("cargo:rerun-if-changed=../../M5EPD/src/utility/GT911.h");
        println!("cargo:rerun-if-changed=../../M5EPD/src/utility/IT8951_Defines.h");
    }
}
