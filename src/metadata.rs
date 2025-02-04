use crate::location::read_locations;
use crate::utils::is_packet_str;
use crate::{location, store};
use cached::cached_result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::SystemTime;
use std::{fs, io};

use super::config;
use super::hash;
use super::utils;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PackitPacket {
    pub id: String,
    pub name: String,
    pub parameters: Option<HashMap<String, serde_json::Value>>,
    pub time: PacketTime,
    pub custom: Option<serde_json::Value>,
}

impl PackitPacket {
    fn from(packet: &Packet) -> PackitPacket {
        PackitPacket {
            id: packet.id.to_string(),
            name: packet.name.to_string(),
            parameters: packet.parameters.clone(),
            time: packet.time.clone(),
            custom: packet.custom.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Packet {
    pub id: String,
    pub name: String,
    pub custom: Option<serde_json::Value>,
    pub parameters: Option<HashMap<String, serde_json::Value>>,
    pub files: Vec<PacketFile>,
    pub depends: Vec<PacketDependency>,
    pub time: PacketTime,
}

impl PartialEq for Packet {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Packet {}

impl std::hash::Hash for Packet {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PacketFile {
    pub path: String,
    pub hash: String,
    pub size: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PacketDependency {
    pub packet: String,
    pub files: Vec<DependencyFile>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PacketTime {
    pub start: f64,
    pub end: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DependencyFile {
    here: String,
    there: String,
}

cached_result! {
    METADATA_CACHE: cached::UnboundCache<PathBuf, Packet> = cached::UnboundCache::new();
    fn read_metadata(path: PathBuf) -> io::Result<Packet> = {
        let file = fs::File::open(path)?;
        let packet: Packet = serde_json::from_reader(file)?;
        Ok(packet)
    }
}

fn get_path(root: &Path, id: &str) -> PathBuf {
    root.join(".outpack").join("metadata").join(id)
}

fn get_metadata_file(root_path: &Path, id: &str) -> io::Result<PathBuf> {
    let path = get_path(root_path, id);
    if !path.exists() {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("packet with id '{}' does not exist", id),
        ))
    } else {
        Ok(path)
    }
}

pub fn get_packit_metadata_from_date(
    root_path: &Path,
    from: Option<f64>,
) -> io::Result<Vec<PackitPacket>> {
    let packets = get_metadata_from_date(root_path, from)?;
    Ok(packets.iter().map(PackitPacket::from).collect())
}

pub fn get_metadata_from_date(root_path: &Path, from: Option<f64>) -> io::Result<Vec<Packet>> {
    let path = root_path.join(".outpack").join("metadata");

    let packets = fs::read_dir(path)?
        .filter_map(|e| e.ok())
        .filter(|e| utils::is_packet(&e.file_name()));

    let mut packets = match from {
        None => packets
            .map(|entry| read_metadata(entry.path()))
            .collect::<io::Result<Vec<Packet>>>()?,
        Some(time) => {
            let location_meta = read_locations(root_path)?;
            packets
                .filter(|entry| {
                    location_meta
                        .iter()
                        .find(|&e| e.packet == entry.file_name().into_string().unwrap())
                        .is_some_and(|e| e.time > time)
                })
                .map(|entry| read_metadata(entry.path()))
                .collect::<io::Result<Vec<Packet>>>()?
        }
    };

    packets.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(packets)
}

pub fn get_metadata_by_id(root_path: &Path, id: &str) -> io::Result<serde_json::Value> {
    let path = get_metadata_file(root_path, id)?;
    let file = fs::File::open(path)?;
    let packet = serde_json::from_reader(file)?;
    Ok(packet)
}

pub fn get_metadata_text(root_path: &Path, id: &str) -> io::Result<String> {
    let path = get_metadata_file(root_path, id)?;
    fs::read_to_string(path)
}

fn get_sorted_id_string(mut ids: Vec<String>) -> String {
    ids.sort();
    ids.join("")
}

pub fn get_ids_digest(root_path: &Path, alg_name: Option<String>) -> io::Result<String> {
    let hash_algorithm = match alg_name {
        None => config::read_config(root_path)?.core.hash_algorithm,
        Some(name) => hash::HashAlgorithm::from_str(&name).map_err(hash::hash_error_to_io_error)?,
    };

    let ids = get_ids(root_path, false)?;
    let id_string = get_sorted_id_string(ids);
    Ok(hash::hash_data(id_string.as_bytes(), hash_algorithm).to_string())
}

pub fn get_ids(root_path: &Path, unpacked: bool) -> io::Result<Vec<String>> {
    let path = root_path.join(".outpack");
    let path = if unpacked {
        path.join("location").join("local")
    } else {
        path.join("metadata")
    };
    Ok(fs::read_dir(path)?
        .filter_map(|r| r.ok())
        .map(|e| e.file_name().into_string())
        .filter_map(|r| r.ok())
        .collect::<Vec<String>>())
}

pub fn get_valid_id(id: &String) -> io::Result<String> {
    let s = id.trim().to_string();
    if is_packet_str(&s) {
        Ok(s)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid packet id '{}'", id),
        ))
    }
}

pub fn get_missing_ids(root: &Path, wanted: &[String], unpacked: bool) -> io::Result<Vec<String>> {
    let known: HashSet<String> = get_ids(root, unpacked)?.into_iter().collect();
    let wanted: HashSet<String> = wanted
        .iter()
        .map(get_valid_id)
        .collect::<io::Result<HashSet<String>>>()?;
    Ok(wanted.difference(&known).cloned().collect::<Vec<String>>())
}

fn check_missing_files(root: &Path, packet: &Packet) -> Result<(), io::Error> {
    let files = packet
        .files
        .iter()
        .map(|f| f.hash.clone())
        .collect::<Vec<String>>();

    let missing_files = store::get_missing_files(root, &files)?;
    if !missing_files.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Can't import metadata for {}, as files missing: \n {}",
                packet.id,
                missing_files.join(",")
            ),
        ));
    }
    Ok(())
}

fn check_missing_dependencies(root: &Path, packet: &Packet) -> Result<(), io::Error> {
    let deps = packet
        .depends
        .iter()
        .map(|d| d.packet.clone())
        .collect::<Vec<String>>();

    let missing_packets = get_missing_ids(root, &deps, true)?;
    if !missing_packets.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Can't import metadata for {}, as dependencies missing: \n {}",
                packet.id,
                missing_packets.join(",")
            ),
        ));
    }
    Ok(())
}

fn add_parsed_metadata(root: &Path, data: &str, packet: &Packet, hash: &str) -> io::Result<()> {
    hash::validate_hash_data(data.as_bytes(), hash).map_err(hash::hash_error_to_io_error)?;
    let path = get_path(root, &packet.id);
    if !path.exists() {
        fs::File::create(&path)?;
        fs::write(path, data)?;
    }
    Ok(())
}

/// Add metadata to the repository.
#[cfg(test)] // Only used from tests at the moment.
pub fn add_metadata(root: &Path, data: &str, hash: &hash::Hash) -> io::Result<()> {
    let packet: Packet = serde_json::from_str(data)?;
    add_parsed_metadata(root, data, &packet, &hash.to_string())
}

/// Add a packet to the repository.
///
/// The packet's files and dependencies must already be present in the repository.
pub fn add_packet(root: &Path, data: &str, hash: &hash::Hash) -> io::Result<()> {
    let packet: Packet = serde_json::from_str(data)?;
    let hash_str = hash.to_string();

    check_missing_files(root, &packet)?;
    check_missing_dependencies(root, &packet)?;

    add_parsed_metadata(root, data, &packet, &hash.to_string())?;

    let time = SystemTime::now();
    location::mark_packet_known(&packet.id, "local", &hash_str, time, root)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::file_exists;
    use crate::test_utils::tests::{get_temp_outpack_root, start_packet};
    use crate::utils::time_as_num;
    use md5::Md5;
    use serde_json::Value;
    use sha2::{Digest, Sha256};

    #[test]
    fn can_get_packets_from_date() {
        let all_packets = get_metadata_from_date(Path::new("tests/example"), None).unwrap();
        assert_eq!(all_packets.len(), 4);
        let recent_packets =
            get_metadata_from_date(Path::new("tests/example"), Some(1662480556.)).unwrap();
        assert_eq!(recent_packets.len(), 1);
        assert_eq!(
            recent_packets.first().unwrap().id,
            "20170818-164847-7574883b"
        );

        let recent_packets =
            get_metadata_from_date(Path::new("tests/example"), Some(1662480555.)).unwrap();
        assert_eq!(recent_packets.len(), 4);
    }

    #[test]
    fn can_get_packet() {
        let _packet =
            get_metadata_by_id(Path::new("tests/example"), "20180818-164043-7cdcde4b").unwrap();
    }

    #[test]
    fn ids_are_sorted() {
        let ids = vec![
            String::from("20180818-164847-7574883b"),
            String::from("20170818-164847-7574883b"),
            String::from("20170819-164847-7574883b"),
            String::from("20170819-164847-7574883a"),
        ];
        let id_string = get_sorted_id_string(ids);
        assert_eq!(
            id_string,
            "20170818-164847-7574883b20170819-164847-7574883a\
        20170819-164847-7574883b20180818-164847-7574883b"
        )
    }

    #[test]
    fn can_get_ids_digest_with_config_alg() {
        let digest = get_ids_digest(Path::new("tests/example"), None).unwrap();
        let dat = "20170818-164830-33e0ab0120170818-164847-7574883b20180220-095832-16a4bbed\
        20180818-164043-7cdcde4b";
        let expected = format!("sha256:{:x}", Sha256::digest(dat));
        assert_eq!(digest, expected);
    }

    #[test]
    fn can_get_ids_digest_with_given_alg() {
        let digest = get_ids_digest(Path::new("tests/example"), Some(String::from("md5"))).unwrap();
        let dat = "20170818-164830-33e0ab0120170818-164847-7574883b20180220-095832-16a4bbed\
        20180818-164043-7cdcde4b";
        let expected = format!("md5:{:x}", Md5::digest(dat));
        assert_eq!(digest, expected);
    }

    #[test]
    fn can_get_ids() {
        let ids = get_ids(Path::new("tests/example"), false).unwrap();
        assert_eq!(ids.len(), 4);
        assert!(ids.iter().any(|e| e == "20170818-164830-33e0ab01"));
        assert!(ids.iter().any(|e| e == "20170818-164847-7574883b"));
        assert!(ids.iter().any(|e| e == "20180220-095832-16a4bbed"));
        assert!(ids.iter().any(|e| e == "20180818-164043-7cdcde4b"));
    }

    #[test]
    fn can_get_unpacked_ids() {
        let ids = get_ids(Path::new("tests/example"), true).unwrap();
        assert_eq!(ids.len(), 1);
        assert!(ids.iter().any(|e| e == "20170818-164847-7574883b"));
    }

    #[test]
    fn can_get_missing_ids() {
        let ids = get_missing_ids(
            Path::new("tests/example"),
            &[
                "20180818-164043-7cdcde4b".to_string(),
                "20170818-164830-33e0ab02".to_string(),
            ],
            false,
        )
        .unwrap();
        assert_eq!(ids.len(), 1);
        assert!(ids.iter().any(|e| e == "20170818-164830-33e0ab02"));

        // check whitespace insensitivity
        let ids = get_missing_ids(
            Path::new("tests/example"),
            &[
                "20180818-164043-7cdcde4b".to_string(),
                "20170818-164830-33e0ab02".to_string(),
            ],
            false,
        )
        .unwrap();
        assert_eq!(ids.len(), 1);
        assert!(ids.iter().any(|e| e == "20170818-164830-33e0ab02"));
    }

    #[test]
    fn can_get_missing_unpacked_ids() {
        let ids = get_missing_ids(
            Path::new("tests/example"),
            &[
                "20170818-164847-7574883b".to_string(),
                "20170818-164830-33e0ab02".to_string(),
            ],
            true,
        )
        .unwrap();
        assert_eq!(ids.len(), 1);
        assert!(ids.iter().any(|e| e == "20170818-164830-33e0ab02"));
    }

    #[test]
    fn bad_ids_raise_error() {
        let res = get_missing_ids(
            Path::new("tests/example"),
            &[
                "20180818-164043-7cdcde4b".to_string(),
                "20170818-164830-33e0ab0".to_string(),
            ],
            false,
        )
        .map_err(|e| e.kind());
        assert_eq!(Err(io::ErrorKind::InvalidInput), res);
    }

    #[test]
    fn can_add_packet() {
        let data = r#"{
                             "schema_version": "0.0.1",
                              "name": "computed-resource",
                              "id": "20230427-150828-68772cee",
                              "time": {
                                "start": 1682608108.4139,
                                "end": 1682608108.4309
                              },
                              "parameters": null,
                              "files": [
                               {
                                  "path": "data.csv",
                                  "size": 51,
                                  "hash": "sha256:b189579a9326f585d308304bd9e03326be5d395ac71b31df359ab8bac408d248"
                                }],
                              "depends": [{
                                  "packet": "20170818-164847-7574883b",
                                  "files": []
                              }],
                              "script": [
                                "orderly.R"
                              ]
                            }"#;
        let hash = hash::hash_data(data.as_bytes(), hash::HashAlgorithm::Sha256);
        let root = get_temp_outpack_root();
        add_packet(&root, data, &hash).unwrap();
        let packet = get_metadata_by_id(&root, "20230427-150828-68772cee").unwrap();
        let expected: Value = serde_json::from_str(data).unwrap();
        assert_eq!(packet, expected);
    }

    #[test]
    fn add_packet_is_idempotent() {
        let data = r#"{
                             "schema_version": "0.0.1",
                              "name": "computed-resource",
                              "id": "20230427-150828-68772cee",
                              "time": {
                                "start": 1682608108.4139,
                                "end": 1682608108.4309
                              },
                              "parameters": null,
                              "files": [],
                              "depends": [],
                              "script": [
                                "orderly.R"
                              ]
                            }"#;
        let hash = hash::hash_data(data.as_bytes(), hash::HashAlgorithm::Sha256);
        let root = get_temp_outpack_root();
        add_packet(&root, data, &hash).unwrap();
        let packet = get_metadata_by_id(&root, "20230427-150828-68772cee").unwrap();
        let expected: Value = serde_json::from_str(data).unwrap();
        assert_eq!(packet, expected);
        add_packet(&root, data, &hash).unwrap();
    }

    #[test]
    fn imported_metadata_is_added_to_local_location() {
        let data = r#"{
                             "schema_version": "0.0.1",
                              "name": "computed-resource",
                              "id": "20230427-150828-68772cee",
                              "time": {
                                "start": 1682608108.4139,
                                "end": 1682608108.4309
                              },
                              "parameters": null,
                              "files": [],
                              "depends": [],
                              "script": [
                                "orderly.R"
                              ]
                            }"#;
        let hash = hash::hash_data(data.as_bytes(), hash::HashAlgorithm::Sha256);
        let root = get_temp_outpack_root();
        let now = SystemTime::now();
        add_packet(&root, data, &hash).unwrap();
        let path = Path::new(&root)
            .join(".outpack")
            .join("location")
            .join("local");
        let entries = location::read_location(path).unwrap();
        let entry = entries
            .iter()
            .find(|l| l.packet == "20230427-150828-68772cee")
            .unwrap();
        assert_eq!(entry.packet, "20230427-150828-68772cee");
        assert_eq!(entry.hash, hash.to_string());
        println!("time {} now {}", entry.time, time_as_num(now));
        assert!(entry.time >= time_as_num(now));
    }

    #[test]
    fn can_add_metadata_with_missing_files() {
        let root = get_temp_outpack_root();

        let file_hash = "sha256:c7b512b2d14a7caae8968830760cb95980a98e18ca2c2991b87c71529e223164";

        assert!(!file_exists(&root, file_hash).unwrap());

        let (_, metadata, hash) = start_packet("data")
            .add_file("data.csv", file_hash, 51)
            .finish();

        add_metadata(&root, &metadata, &hash).unwrap();
    }

    #[test]
    fn cannot_add_packet_with_missing_files() {
        let root = get_temp_outpack_root();

        let file_hash = "sha256:c7b512b2d14a7caae8968830760cb95980a98e18ca2c2991b87c71529e223164";

        assert!(!file_exists(&root, file_hash).unwrap());

        let (_, metadata, hash) = start_packet("data")
            .add_file("data.csv", file_hash, 51)
            .finish();

        let res = add_packet(&root, &metadata, &hash);
        assert_regex!(
            res.unwrap_err().to_string(),
            "Can't import metadata for .*, as files missing:"
        );
    }

    #[test]
    fn cannot_add_packet_with_missing_dependencies() {
        let (dependency_id, _, _) = start_packet("upstream").finish();
        let (_, metadata, hash) = start_packet("downstream")
            .add_dependency(dependency_id, vec![])
            .finish();

        let root = get_temp_outpack_root();

        let res = add_packet(&root, &metadata, &hash);
        assert_regex!(
            res.unwrap_err().to_string(),
            "Can't import metadata for .*, as dependencies missing:"
        );
    }
}
