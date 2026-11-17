//! Shared loader for logical JSONL examples, including numbered fragments.

use std::fs;
use std::io;
use std::path::Path;

/// Load sorted logical recordings, concatenating numbered parts in order.
pub fn recordings(dir: &Path) -> io::Result<Vec<(String, Vec<u8>)>> {
    let mut paths = fs::read_dir(dir)?.map(|entry| entry.map(|e| e.path())).collect::<Result<Vec<_>, _>>()?;
    paths.retain(|path| path.extension().and_then(|ext| ext.to_str()) == Some("jsonl"));
    paths.sort();
    let mut grouped = std::collections::BTreeMap::<String, (Vec<u8>, u8, bool)>::new();
    for path in paths {
        let name = path.file_name().and_then(|name| name.to_str()).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid example filename"))?;
        if let Some((logical, index)) = fragment_name(name) {
            let logical_name = format!("{logical}.jsonl");
            let entry = grouped.entry(logical_name.clone()).or_insert_with(|| (Vec::new(), 0, true));
            if !entry.2 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, format!("mixed monolithic and fragmented recording: {logical_name}")));
            }
            if index != entry.1 + 1 { return Err(io::Error::new(io::ErrorKind::InvalidData, format!("nonconsecutive fragment: {name}"))); }
            entry.0.extend(fs::read(path)?);
            entry.1 = index;
        } else {
            let entry = grouped.entry(name.to_string()).or_insert_with(|| (Vec::new(), 0, false));
            if entry.2 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, format!("mixed monolithic and fragmented recording: {name}")));
            }
            entry.0.extend(fs::read(path)?);
        }
    }
    Ok(grouped.into_iter().map(|(name, (bytes, _, _))| (name, bytes)).collect())
}

fn fragment_name(name: &str) -> Option<(&str, u8)> {
    let (stem, suffix) = name.strip_suffix(".jsonl")?.rsplit_once('-')?;
    if suffix.len() != 2 { return None; }
    let index = suffix.parse::<u8>().ok()?;
    if index == 0 { return None; }
    Some((stem, index))
}
