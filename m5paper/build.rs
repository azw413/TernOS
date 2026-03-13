fn main() {
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/allocator.rs");
    println!("cargo:rerun-if-changed=src/ffi.rs");
    println!("cargo:rerun-if-changed=src/display.rs");
    println!("cargo:rerun-if-changed=src/image_source.rs");
    println!("cargo:rerun-if-changed=src/runtime_host.rs");
}
