fn main() {
    generate_embedded_prc_fonts();
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
    let fonts_dir = Path::new("..").join("x4").join("assets").join("fonts");

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
