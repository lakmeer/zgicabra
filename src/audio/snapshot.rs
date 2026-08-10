
//
// Mod matrix snapshotting: dumps every ParamSpec cell to a small text file
// under snapshots/, one new timestamped file per save (never overwrites),
// and reloads one back into a live ModMatrix. No serde anywhere in this
// project -- it's a flat name.cell=value list, plain enough not to need one.
//

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::AudioHandles;

const SNAPSHOT_DIR: &str = "snapshots";

pub fn save_snapshot (audio: &AudioHandles) -> io::Result<PathBuf> {
    fs::create_dir_all(SNAPSHOT_DIR)?;

    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let path = Path::new(SNAPSHOT_DIR).join(format!("{stamp}.snap"));

    let mut text = String::new();
    for spec in audio.mod_matrix.entries() {
        for (cell_name, cell) in spec.cells() {
            text.push_str(&format!("{}.{}={}\n", spec.name, cell_name, cell.value()));
        }
    }

    for (row_name, cycler) in [("gen_1", &audio.gen_1), ("gen_2", &audio.gen_2), ("gen_3", &audio.gen_3), ("gen_4", &audio.gen_4)] {
        for (cell_name, cell) in cycler.cells() {
            text.push_str(&format!("{row_name}.{cell_name}={}\n", cell.value()));
        }
    }

    for (row_name, cycler) in [("fx_1", &audio.fx_1), ("fx_2", &audio.fx_2), ("fx_3", &audio.fx_3), ("fx_4", &audio.fx_4)] {
        for (cell_name, cell) in cycler.cells() {
            text.push_str(&format!("{row_name}.{cell_name}={}\n", cell.value()));
        }
    }

    fs::write(&path, text)?;
    Ok(path)
}

pub fn load_snapshot (path: &Path, audio: &AudioHandles) -> io::Result<()> {
    let text = fs::read_to_string(path)?;

    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        let Some((row_name, cell_name)) = key.split_once('.') else { continue };
        let Ok(value) = value.parse::<f32>() else { continue };

        let gen_cyclers = [("gen_1", &audio.gen_1), ("gen_2", &audio.gen_2), ("gen_3", &audio.gen_3), ("gen_4", &audio.gen_4)];
        if let Some((_, cycler)) = gen_cyclers.into_iter().find(|(name, _)| *name == row_name) {
            for (name, cell) in cycler.cells() {
                if name == cell_name { cell.set_value(value); }
            }
            continue;
        }

        let fx_cyclers = [("fx_1", &audio.fx_1), ("fx_2", &audio.fx_2), ("fx_3", &audio.fx_3), ("fx_4", &audio.fx_4)];
        if let Some((_, cycler)) = fx_cyclers.into_iter().find(|(name, _)| *name == row_name) {
            for (name, cell) in cycler.cells() {
                if name == cell_name { cell.set_value(value); }
            }
            continue;
        }

        for spec in audio.mod_matrix.entries() {
            if spec.name != row_name { continue; }
            for (name, cell) in spec.cells() {
                if name == cell_name { cell.set_value(value); }
            }
        }
    }

    Ok(())
}

// Discovers every *.snap file in SNAPSHOT_DIR (sorted -- timestamped names
// sort oldest-first, same stable-cycle-order convention as
// nam::load_nam_models).
pub fn list_snapshots () -> io::Result<Vec<String>> {
    fs::create_dir_all(SNAPSHOT_DIR)?;

    let mut names: Vec<String> = fs::read_dir(SNAPSHOT_DIR)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("snap")))
        .filter_map(|path| path.file_stem().and_then(|s| s.to_str()).map(String::from))
        .collect();
    names.sort();
    Ok(names)
}

pub fn snapshot_path (name: &str) -> PathBuf {
    Path::new(SNAPSHOT_DIR).join(format!("{name}.snap"))
}
