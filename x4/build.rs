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
    use std::collections::BTreeMap;

    #[derive(Clone)]
    struct ParsedFont {
        font_id: u16,
        first_char: u8,
        last_char: u8,
        max_width: u8,
        avg_width: u8,
        rect_height: u8,
        widths: Vec<u8>,
        glyphs: Vec<Option<(u8, Vec<u16>)>>,
    }

    fn parse_font_resource_id_from_name(file_name: &str) -> Option<u16> {
        let mut nums: Vec<u16> = Vec::new();
        let mut cur = String::new();
        for ch in file_name.chars() {
            if ch.is_ascii_digit() {
                cur.push(ch);
            } else if !cur.is_empty() {
                if let Ok(v) = cur.parse::<u16>() {
                    nums.push(v);
                }
                cur.clear();
            }
        }
        if !cur.is_empty() {
            if let Ok(v) = cur.parse::<u16>() {
                nums.push(v);
            }
        }
        for v in nums {
            if (9000..=9099).contains(&v) {
                return Some(v.saturating_add(100));
            }
            if (9100..=9999).contains(&v) {
                return Some(v);
            }
            if v <= 255 {
                return Some(9100u16.saturating_add(v));
            }
        }
        None
    }

    fn parse_pumpkin_txt_font(text: &str, font_id: u16) -> Option<ParsedFont> {
        let mut ascent: u8 = 0;
        let mut descent: u8 = 0;
        let mut glyphs: BTreeMap<u8, (u8, Vec<u16>)> = BTreeMap::new();

        let mut lines = text.lines().peekable();
        while let Some(raw_line) = lines.next() {
            let line = raw_line.trim();
            if let Some(rest) = line.strip_prefix("ascent ") {
                if let Ok(v) = rest.trim().parse::<u8>() {
                    ascent = v;
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("descent ") {
                if let Ok(v) = rest.trim().parse::<u8>() {
                    descent = v;
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("GLYPH ") {
                let Ok(code_u16) = rest.trim().parse::<u16>() else {
                    continue;
                };
                if code_u16 > 255 {
                    continue;
                }
                let code = code_u16 as u8;
                let mut rows: Vec<&str> = Vec::new();
                while let Some(row) = lines.peek().copied() {
                    let row_trim = row.trim();
                    if row_trim.is_empty() || row_trim.starts_with("GLYPH ") {
                        break;
                    }
                    rows.push(row);
                    let _ = lines.next();
                }
                let mut width = 0usize;
                let mut row_bits: Vec<u16> = Vec::new();
                for row in &rows {
                    let bytes = row.as_bytes();
                    let mut last_hash: Option<usize> = None;
                    let mut bits = 0u16;
                    for (idx, b) in bytes.iter().enumerate() {
                        if *b == b'#' {
                            last_hash = Some(idx);
                            if idx < 16 {
                                bits |= 1u16 << idx;
                            }
                        }
                    }
                    let w = if let Some(last) = last_hash { last + 1 } else { bytes.len() };
                    width = width.max(w);
                    row_bits.push(bits);
                }
                glyphs.insert(code, (width.min(255) as u8, row_bits));
                continue;
            }
        }

        if glyphs.is_empty() {
            return None;
        }
        let first_char = *glyphs.keys().next()?;
        let last_char = *glyphs.keys().next_back()?;
        let mut widths = vec![0u8; (last_char - first_char) as usize + 1];
        let mut bitmaps = vec![None; widths.len()];
        for (ch, (w, rows)) in glyphs {
            let idx = (ch - first_char) as usize;
            widths[idx] = w.max(1);
            bitmaps[idx] = Some((w.max(1), rows));
        }
        let max_width = widths.iter().copied().max().unwrap_or(1).max(1);
        let sum: u32 = widths.iter().map(|w| *w as u32).sum();
        let avg_width = ((sum / widths.len().max(1) as u32) as u8).max(1);
        let rect_height = ascent.saturating_add(descent).max(1);

        Some(ParsedFont {
            font_id,
            first_char,
            last_char,
            max_width,
            avg_width,
            rect_height,
            widths,
            glyphs: bitmaps,
        })
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR missing"));
    let out_file = out_dir.join("prc_embedded_fonts.rs");
    let fonts_dir = Path::new("assets").join("fonts");

    println!("cargo:rerun-if-changed={}", fonts_dir.display());

    #[derive(Default)]
    struct FontVariants {
        f72: Option<(String, PathBuf)>,
        f144: Option<(String, PathBuf)>,
        other: Option<(String, PathBuf)>,
    }

    let mut variants: BTreeMap<u16, FontVariants> = BTreeMap::new();
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
            let Some(resource_id) = parse_font_resource_id_from_name(&name) else {
                continue;
            };
            let font_id = resource_id.saturating_sub(9100);
            if !matches!(font_id, 0 | 1 | 2 | 7) {
                continue;
            }
            let lower = name.to_ascii_lowercase();
            let slot = variants.entry(font_id).or_default();
            if lower.ends_with("_72.txt") {
                slot.f72 = Some((name, path));
            } else if lower.ends_with("_144.txt") {
                slot.f144 = Some((name, path));
            } else if slot.other.is_none() {
                slot.other = Some((name, path));
            }
        }
    }

    fn choose_variant(
        variants: &FontVariants,
        prefer_144: bool,
    ) -> Option<(String, PathBuf)> {
        if prefer_144 {
            variants
                .f144
                .clone()
                .or_else(|| variants.other.clone())
                .or_else(|| variants.f72.clone())
        } else {
            variants
                .f72
                .clone()
                .or_else(|| variants.other.clone())
                .or_else(|| variants.f144.clone())
        }
    }

    let mut chosen_72: BTreeMap<u16, (String, PathBuf)> = BTreeMap::new();
    let mut chosen_144: BTreeMap<u16, (String, PathBuf)> = BTreeMap::new();
    for (font_id, v) in &variants {
        if let Some(chosen) = choose_variant(v, false) {
            chosen_72.insert(*font_id, chosen);
        }
        if let Some(chosen) = choose_variant(v, true) {
            chosen_144.insert(*font_id, chosen);
        }
    }

    let mut body = String::new();
    body.push_str("use tern_core::palm::runtime::{PalmFont, PalmWidths, PalmGlyphs, PalmGlyphStatic};\n");
    let mut emitted: BTreeMap<String, u16> = BTreeMap::new();
    for (tag, map) in [("72", &chosen_72), ("144", &chosen_144)] {
        for (font_id, (_name, path)) in map.iter() {
            let Ok(text) = fs::read_to_string(path) else {
                continue;
            };
            let Some(parsed) = parse_pumpkin_txt_font(&text, *font_id) else {
                continue;
            };
            let prefix = format!("F{}_{}", font_id, tag);
            if emitted.contains_key(&prefix) {
                continue;
            }
            emitted.insert(prefix.clone(), *font_id);
        body.push_str(&format!("static {}_WIDTHS: &[u8] = &{:?};\n", prefix, parsed.widths));
        for (idx, g) in parsed.glyphs.iter().enumerate() {
            if let Some((_, rows)) = g {
                body.push_str(&format!("static {}_G{}_ROWS: &[u16] = &{:?};\n", prefix, idx, rows));
            }
        }
        body.push_str(&format!("static {}_GLYPHS: &[Option<PalmGlyphStatic>] = &[\n", prefix));
        for (idx, g) in parsed.glyphs.iter().enumerate() {
            if let Some((w, _rows)) = g {
                body.push_str(&format!(
                    "    Some(PalmGlyphStatic {{ width: {}, rows: {}_G{}_ROWS }}),\n",
                    w, prefix, idx
                ));
            } else {
                body.push_str("    None,\n");
            }
        }
        body.push_str("];\n");
        body.push_str(&format!(
            "fn make_{}() -> PalmFont {{ PalmFont {{ font_id: {}, first_char: {}, last_char: {}, max_width: {}, avg_width: {}, rect_height: {}, widths: PalmWidths::Static({}_WIDTHS), glyphs: PalmGlyphs::Static({}_GLYPHS) }} }}\n",
            prefix.to_ascii_lowercase(),
            parsed.font_id,
            parsed.first_char,
            parsed.last_char,
            parsed.max_width,
            parsed.avg_width,
            parsed.rect_height,
            prefix,
            prefix
        ));
        }
    }
    body.push_str("pub fn load_embedded_prc_fonts_72() -> alloc::vec::Vec<PalmFont> {\n");
    body.push_str("    let mut out = alloc::vec::Vec::new();\n");
    for font_id in chosen_72.keys() {
        let prefix = format!("F{}_72", font_id).to_ascii_lowercase();
        body.push_str(&format!("    out.push(make_{}());\n", prefix));
    }
    body.push_str("    out\n}\n");
    body.push_str("pub fn load_embedded_prc_fonts_144() -> alloc::vec::Vec<PalmFont> {\n");
    body.push_str("    let mut out = alloc::vec::Vec::new();\n");
    for font_id in chosen_144.keys() {
        let prefix = format!("F{}_144", font_id).to_ascii_lowercase();
        body.push_str(&format!("    out.push(make_{}());\n", prefix));
    }
    body.push_str("    out\n}\n");
    body.push_str("pub fn load_embedded_prc_fonts() -> alloc::vec::Vec<PalmFont> {\n");
    body.push_str("    load_embedded_prc_fonts_72()\n");
    body.push_str("}\n");

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
