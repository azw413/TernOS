use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn usage() {
    eprintln!("Usage: tern-prc-fonts <input.prc|input.pdb|input.bin> [out_dir]");
    eprintln!("Example: tern-prc-fonts ../PumpkinOS/rom.pdb sdcard/fonts");
}

fn read_file(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("failed to read {}: {}", path.display(), e))
}

fn write_font(out_dir: &Path, id: u16, data: &[u8]) -> Result<PathBuf, String> {
    fs::create_dir_all(out_dir)
        .map_err(|e| format!("failed to create {}: {}", out_dir.display(), e))?;
    let path = out_dir.join(format!("NFNT_{}.nfnt", id));
    fs::write(&path, data).map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
    Ok(path)
}

fn main() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(input) = args.next() else {
        usage();
        return Err("missing input path".into());
    };
    let out_dir = args.next().unwrap_or_else(|| "sdcard/fonts".to_string());

    let input_path = PathBuf::from(input);
    let out_path = PathBuf::from(out_dir);
    let raw = read_file(&input_path)?;
    let info = tern_core::palm::parse_prc(&raw)
        .ok_or_else(|| format!("{} is not a parseable PRC/PDB resource DB", input_path.display()))?;

    let mut extracted = 0usize;
    for res in &info.resources {
        if res.kind != "NFNT" {
            continue;
        }
        let start = res.offset as usize;
        let end = start.saturating_add(res.size as usize);
        let Some(data) = raw.get(start..end) else {
            continue;
        };
        let written = write_font(&out_path, res.id, data)?;
        println!("extracted NFNT#{} -> {}", res.id, written.display());
        extracted += 1;
    }

    if extracted == 0 {
        println!(
            "no NFNT resources found in {} (type={} creator={} entries={})",
            input_path.display(),
            info.type_code,
            info.creator_code,
            info.entry_count
        );
    } else {
        println!("done: extracted {} font resources", extracted);
    }

    Ok(())
}
