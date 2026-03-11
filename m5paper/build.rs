fn main() {
    linker_be_nice();
    println!("cargo:rustc-link-arg=-Tlinkall.x");

    if std::env::var_os("CARGO_FEATURE_CSHIM").is_some() {
        println!("cargo:rerun-if-changed=cshim/m5paper_bridge.h");
        println!("cargo:rerun-if-changed=cshim/m5paper_bridge_stub.c");
        cc::Build::new()
            .file("cshim/m5paper_bridge_stub.c")
            .include("cshim")
            .compile("m5paper_bridge");
    }
}

fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 2 {
        let kind = &args[1];
        let what = &args[2];

        match kind.as_str() {
            "undefined-symbol" if what == "_stack_start" => {
                eprintln!();
                eprintln!("💡 Is the linker script `linkall.x` missing?");
                eprintln!();
            }
            _ => {}
        }
    }
}
