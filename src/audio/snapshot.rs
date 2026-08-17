
//
// Voice param snapshotting: dumps one voice's live params to a small text
// file under snapshots/, named "{voice_name}_{index}.snap" with a monotonic
// per-voice index (never overwrites), and reloads one back. No serde
// anywhere in this project -- it's a flat name=value list, plain enough not
// to need one.
//
// This file also owns `config/` -- the always-current, overwriting
// counterpart to the numbered snapshots above: one `{voice_name}.state` file
// per voice plus a `selected` file naming the last-active voice, both
// version-controlled in the repo (not `~/.local/state/`) so tuned params
// survive a crash/restart on the performance box and travel with git. See
// audio/mod.rs's `persist_dirty_voices`/`AudioOutput::new` for who calls
// these and when.
//

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const SNAPSHOT_DIR: &str = "snapshots";
const STATE_DIR: &str = "config";
const SELECTED_FILE: &str = "selected";

// Next free index for this voice -- scans snapshots/ for existing
// "{voice_name}_NNNN.snap" files and returns max+1 (0 if none yet).
fn next_index (voice_name: &str) -> io::Result<u32> {
    fs::create_dir_all(SNAPSHOT_DIR)?;
    let prefix = format!("{voice_name}_");

    let max = fs::read_dir(SNAPSHOT_DIR)?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.path().file_stem().and_then(|s| s.to_str()).map(String::from))
        .filter_map(|stem| stem.strip_prefix(&prefix).and_then(|n| n.parse::<u32>().ok()))
        .max();

    Ok(max.map_or(0, |m| m + 1))
}

pub fn save_snapshot (voice_name: &str, fields: &[(&'static str, f32)]) -> io::Result<PathBuf> {
    fs::create_dir_all(SNAPSHOT_DIR)?;

    let index = next_index(voice_name)?;
    let path = Path::new(SNAPSHOT_DIR).join(format!("{voice_name}_{index:04}.snap"));

    let mut text = String::new();
    for (name, value) in fields {
        text.push_str(&format!("{name}={value}\n"));
    }

    fs::write(&path, text)?;
    Ok(path)
}

// Returns (voice_name, [(param_name, value)]) -- voice_name is parsed back
// out of the filename ("{voice_name}_{index}"), not stored in the file body.
pub fn load_snapshot (path: &Path) -> io::Result<(String, Vec<(String, f32)>)> {
    let text = fs::read_to_string(path)?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let voice_name = stem.rsplit_once('_').map_or(stem, |(name, _)| name).to_string();

    let fields = text.lines()
        .filter_map(|line| line.split_once('='))
        .filter_map(|(key, value)| value.parse::<f32>().ok().map(|v| (key.to_string(), v)))
        .collect();

    Ok((voice_name, fields))
}

// Discovers every *.snap file in SNAPSHOT_DIR (sorted -- zero-padded index
// suffixes sort correctly as long as a single voice stays under 10000 saves).
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

// Overwrites `config/{voice_name}.state` with this voice's current fields --
// unlike save_snapshot, there's only ever one live copy, no monotonic index.
pub fn save_state (voice_name: &str, fields: &[(&'static str, f32)]) -> io::Result<()> {
    fs::create_dir_all(STATE_DIR)?;

    let mut text = String::new();
    for (name, value) in fields {
        text.push_str(&format!("{name}={value}\n"));
    }

    fs::write(Path::new(STATE_DIR).join(format!("{voice_name}.state")), text)
}

// Err(NotFound) on a fresh checkout / first run -- caller treats that as
// "keep this voice's compiled-in defaults", not an error.
pub fn load_state (voice_name: &str) -> io::Result<Vec<(String, f32)>> {
    let text = fs::read_to_string(Path::new(STATE_DIR).join(format!("{voice_name}.state")))?;

    Ok(text.lines()
        .filter_map(|line| line.split_once('='))
        .filter_map(|(key, value)| value.parse::<f32>().ok().map(|v| (key.to_string(), v)))
        .collect())
}

pub fn save_selected (voice_name: &str) -> io::Result<()> {
    fs::create_dir_all(STATE_DIR)?;
    fs::write(Path::new(STATE_DIR).join(SELECTED_FILE), voice_name)
}

// None if the file doesn't exist yet (first run) -- caller falls back to
// index 0.
pub fn load_selected () -> Option<String> {
    fs::read_to_string(Path::new(STATE_DIR).join(SELECTED_FILE)).ok().map(|s| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Save/load round-trip plus the monotonic-index naming -- the only
    // branching logic in this file (index scanning, filename parsing).
    // Uses a "__test_voice" prefix so it can't collide with real growl_*/
    // basic_* snapshots saved by the GUI.
    #[test]
    fn save_load_round_trip_and_monotonic_index () {
        let voice = "__test_voice";
        for path in fs::read_dir(SNAPSHOT_DIR).unwrap() {
            let path = path.unwrap().path();
            if path.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s.starts_with(voice)) {
                let _ = fs::remove_file(path);
            }
        }

        let path0 = save_snapshot(voice, &[("a", 1.0), ("b", 2.5)]).unwrap();
        assert_eq!(path0.file_stem().unwrap().to_str().unwrap(), "__test_voice_0000");

        let path1 = save_snapshot(voice, &[("a", 3.0), ("b", 4.5)]).unwrap();
        assert_eq!(path1.file_stem().unwrap().to_str().unwrap(), "__test_voice_0001");

        let (loaded_voice, fields) = load_snapshot(&path1).unwrap();
        assert_eq!(loaded_voice, voice);
        assert_eq!(fields, vec![("a".to_string(), 3.0), ("b".to_string(), 4.5)]);

        let _ = fs::remove_file(path0);
        let _ = fs::remove_file(path1);
    }
}
