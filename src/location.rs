use serde::{Deserialize, Serialize};
use std::{fs, io};
use std::fs::{DirEntry};
use std::path::{Path, PathBuf};
use regex::Regex;

extern crate walkdir;
use walkdir::WalkDir;

#[derive(Serialize, Deserialize, Debug)]
pub struct LocationEntry {
    packet: String,
    time: f32,
    hash: String,
}

const ID_REG: &'static str = "^([0-9]{8}-[0-9]{6}-[[:xdigit:]]{8})$";

fn read_entry(path: PathBuf) -> io::Result<LocationEntry> {
    let file = fs::File::open(path)?;
    let entry: LocationEntry = serde_json::from_reader(file)?;
    return Ok(entry);
}

fn file_name(entry: io::Result<DirEntry>) -> Option<String> {
    return entry.ok()?.path().file_name()
            .and_then(|n| n.to_str().map(|s| String::from(s)));
}

fn file_name_w(entry: walkdir::Result<walkdir::DirEntry>) -> Option<String> {
    return entry.ok()?.path().file_name()
        .and_then(|n| n.to_str().map(|s| String::from(s)));
}

pub fn read_location(location_id: &str, root_path: &str) -> io::Result<Vec<LocationEntry>> {
    let path = Path::new(root_path)
        .join(".outpack")
        .join("location")
        .join(location_id);

    let reg = Regex::new(ID_REG).unwrap();
    let packets = fs::read_dir(path.clone())?
        .filter_map(|entry| file_name(entry))
        .filter(|s| reg.is_match(s))
        .map(|p| read_entry(path.join(p)))
        .collect::<io::Result<Vec<LocationEntry>>>()?;
    return Ok(packets);
}

fn is_packet(entry: &walkdir::DirEntry) -> bool {
    let reg = Regex::new(ID_REG).unwrap();
    return entry.file_name()
        .to_str()
        .map(|s| reg.is_match(s))
        .unwrap_or(false)
}

pub fn read_locations(root_path: &str) -> io::Result<Vec<LocationEntry>> {
    let path = Path::new(root_path)
        .join(".outpack")
        .join("location");

    let packets = WalkDir::new(path.clone())
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| is_packet(e))
        .map(|entry| read_entry(entry.into_path()))
        .collect::<io::Result<Vec<LocationEntry>>>()?;

    return Ok(packets);
}
