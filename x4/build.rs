fn main() {
    linker_be_nice();
    // make sure linkall.x is the last linker script (otherwise might cause problems with flip-link)
    println!("cargo:rustc-link-arg=-Tlinkall.x");
    generate_embedded_prc_fonts();

    cc::Build::new()
        .compiler("riscv32-esp-elf-gcc")
        .file("fatfs/ff.c")
        .file("fatfs/ffsystem.c")
        .file("fatfs/ffunicode.c")
        .file("fatfs/compat.c")
        .compile("fatfs");
    println!("cargo:rerun-if-changed=fatfs/ff.c");
    println!("cargo:rerun-if-changed=fatfs/ffsystem.c");
    println!("cargo:rerun-if-changed=fatfs/ffunicode.c");
    println!("cargo:rerun-if-changed=fatfs/compat.c");
}

fn generate_embedded_prc_fonts() {
    use std::fs;
    use std::path::{Path, PathBuf};

    fn esc(s: &str) -> String {
        let mut out = String::with_capacity(s.len() + 8);
        for ch in s.chars() {
            match ch {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                _ => out.push(ch),
            }
        }
        out
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR missing"));
    let out_file = out_dir.join("prc_embedded_fonts.rs");
    let fonts_dir = Path::new("assets").join("fonts");

    println!("cargo:rerun-if-changed={}", fonts_dir.display());

    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    if let Ok(rd) = fs::read_dir(&fonts_dir) {
        for dent in rd.flatten() {
            let path = dent.path();
            let Ok(ft) = dent.file_type() else {
                continue;
            };
            if !ft.is_file() {
                continue;
            }
            let Some(name_os) = path.file_name() else {
                continue;
            };
            let name = name_os.to_string_lossy().to_string();
            if !name.to_ascii_lowercase().ends_with(".txt") {
                continue;
            }
            entries.push((name, path));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
    }

    let mut body = String::new();
    body.push_str("pub const EMBEDDED_PRC_FONT_TXT: &[(&str, &str)] = &[\n");
    for (name, path) in entries {
        let include_path = fs::canonicalize(&path).unwrap_or(path);
        let name_e = esc(&name);
        let path_e = esc(&include_path.to_string_lossy());
        body.push_str("    (\"");
        body.push_str(&name_e);
        body.push_str("\", include_str!(\"");
        body.push_str(&path_e);
        body.push_str("\")),\n");
    }
    body.push_str("];\n");

    let _ = fs::create_dir_all(&out_dir);
    fs::write(out_file, body).expect("failed to write generated embedded font table");
}

fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let kind = &args[1];
        let what = &args[2];

        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                what if what.starts_with("_defmt_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `defmt` not found - make sure `defmt.x` is added as a linker script and you have included `use defmt_rtt as _;`"
                    );
                    eprintln!();
                }
                "_stack_start" => {
                    eprintln!();
                    eprintln!("💡 Is the linker script `linkall.x` missing?");
                    eprintln!();
                }
                what if what.starts_with("esp_rtos_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `esp-radio` has no scheduler enabled. Make sure you have initialized `esp-rtos` or provided an external scheduler."
                    );
                    eprintln!();
                }
                "embedded_test_linker_file_not_added_to_rustflags" => {
                    eprintln!();
                    eprintln!(
                        "💡 `embedded-test` not found - make sure `embedded-test.x` is added as a linker script for tests"
                    );
                    eprintln!();
                }
                "free"
                | "malloc"
                | "calloc"
                | "get_free_internal_heap_size"
                | "malloc_internal"
                | "realloc_internal"
                | "calloc_internal"
                | "free_internal" => {
                    eprintln!();
                    eprintln!(
                        "💡 Did you forget the `esp-alloc` dependency or didn't enable the `compat` feature on it?"
                    );
                    eprintln!();
                }
                _ => (),
            },
            // we don't have anything helpful for "missing-lib" yet
            _ => {
                std::process::exit(1);
            }
        }

        std::process::exit(0);
    }

    println!(
        "cargo:rustc-link-arg=--error-handling-script={}",
        std::env::current_exe().unwrap().display()
    );
}
