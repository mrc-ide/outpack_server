use serde::{Deserialize, Serialize};
use std::{fs, io};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use regex::Regex;
use walkdir::WalkDir;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LocationEntry {
    pub packet: String,
    pub time: f32,
    pub hash: String,
}

const ID_REG: &'static str = "^([0-9]{8}-[0-9]{6}-[[:xdigit:]]{8})$";

cached_result! {
    ENTRY_CACHE: cached::UnboundCache<PathBuf, LocationEntry> = cached::UnboundCache::new();
    fn read_entry(path: PathBuf) -> io::Result<LocationEntry> = {
        let file = fs::File::open(path)?;
        let entry: LocationEntry = serde_json::from_reader(file)?;
        return Ok(entry);
    }
}

fn is_packet(name: &OsStr) -> bool {
    let reg = Regex::new(ID_REG).unwrap();
    return name
        .to_str()
        .map(|s| reg.is_match(s))
        .unwrap_or(false);
}

pub fn read_locations(root_path: &str) -> io::Result<Vec<LocationEntry>> {
    let path = Path::new(root_path)
        .join(".outpack")
        .join("location");

    let packets = WalkDir::new(path.clone())
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| is_packet(e.file_name()))
        .map(|entry| read_entry(entry.into_path()))
        .collect::<io::Result<Vec<LocationEntry>>>()?;

    return Ok(packets);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_detect_packet_id() {
        assert_eq!(is_packet(OsStr::new("1234")), false);
        assert_eq!(is_packet(OsStr::new("20170818-164830-33e0ab01")), true);
    }
}
