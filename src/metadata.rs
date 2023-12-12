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
}

impl PackitPacket {
    fn from(packet: &Packet) -> PackitPacket {
        PackitPacket {
            id: packet.id.to_string(),
            name: packet.name.to_string(),
            parameters: packet.parameters.clone(),
            time: packet.time.clone(),
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
    path: String,
    hash: String,
    size: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PacketDependency {
    packet: String,
    files: Vec<DependencyFile>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PacketTime {
    start: f64,
    end: f64,
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

fn get_path(root_path: &str, id: &str) -> PathBuf {
    Path::new(root_path)
        .join(".outpack")
        .join("metadata")
        .join(id)
}

fn get_metadata_file(root_path: &str, id: &str) -> io::Result<PathBuf> {
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
    root_path: &str,
    from: Option<f64>,
) -> io::Result<Vec<PackitPacket>> {
    let packets = get_metadata_from_date(root_path, from)?;
    Ok(packets.iter().map(PackitPacket::from).collect())
}

pub fn get_metadata_from_date(root_path: &str, from: Option<f64>) -> io::Result<Vec<Packet>> {
    let path = Path::new(root_path).join(".outpack").join("metadata");

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
                        .map_or(false, |e| e.time > time)
                })
                .map(|entry| read_metadata(entry.path()))
                .collect::<io::Result<Vec<Packet>>>()?
        }
    };

    packets.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(packets)
}

pub fn get_metadata_by_id(root_path: &str, id: &str) -> io::Result<serde_json::Value> {
    let path = get_metadata_file(root_path, id)?;
    let file = fs::File::open(path)?;
    let packet = serde_json::from_reader(file)?;
    Ok(packet)
}

pub fn get_metadata_text(root_path: &str, id: &str) -> io::Result<String> {
    let path = get_metadata_file(root_path, id)?;
    fs::read_to_string(path)
}

fn get_sorted_id_string(mut ids: Vec<String>) -> String {
    ids.sort();
    ids.join("")
}

pub fn get_ids_digest(root_path: &str, alg_name: Option<String>) -> io::Result<String> {
    let hash_algorithm = match alg_name {
        None => config::read_config(root_path)?.core.hash_algorithm,
        Some(name) => hash::HashAlgorithm::from_str(&name).map_err(hash::hash_error_to_io_error)?,
    };

    let ids = get_ids(root_path, None)?;
    let id_string = get_sorted_id_string(ids);
    Ok(hash::hash_data(id_string.as_bytes(), hash_algorithm).to_string())
}

pub fn get_ids(root_path: &str, unpacked: Option<bool>) -> io::Result<Vec<String>> {
    let path = Path::new(root_path).join(".outpack");
    let path = if unpacked.is_some_and(|x| x) {
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

pub fn get_missing_ids(
    root_path: &str,
    wanted: &[String],
    unpacked: Option<bool>,
) -> io::Result<Vec<String>> {
    let known: HashSet<String> = get_ids(root_path, unpacked)?.into_iter().collect();
    let wanted: HashSet<String> = wanted
        .iter()
        .map(get_valid_id)
        .collect::<io::Result<HashSet<String>>>()?;
    Ok(wanted.difference(&known).cloned().collect::<Vec<String>>())
}

fn check_missing_files(root: &str, packet: &Packet) -> Result<(), io::Error> {
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

fn check_missing_dependencies(root: &str, packet: &Packet) -> Result<(), io::Error> {
    let deps = packet
        .depends
        .iter()
        .map(|d| d.packet.clone())
        .collect::<Vec<String>>();

    let missing_packets = get_missing_ids(root, &deps, Some(true))?;
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

pub fn add_metadata(root: &str, data: &str, hash: &hash::Hash) -> io::Result<()> {
    let packet: Packet = serde_json::from_str(data)?;
    let hash_str = hash.to_string();

    hash::validate_hash_data(data.as_bytes(), &hash_str).map_err(hash::hash_error_to_io_error)?;
    check_missing_files(root, &packet)?;
    check_missing_dependencies(root, &packet)?;

    let path = get_path(root, &packet.id);

    if !path.exists() {
        fs::File::create(&path)?;
        fs::write(path, data)?;
    }
    let time = SystemTime::now();
    location::mark_packet_known(&packet.id, "local", &hash_str, time, root)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::tests::get_temp_outpack_root;
    use crate::utils::time_as_num;
    use serde_json::Value;
    use sha2::{Digest, Sha256};

    #[test]
    fn can_get_packets_from_date() {
        let all_packets = get_metadata_from_date("tests/example", None).unwrap();
        assert_eq!(all_packets.len(), 4);
        let recent_packets =
            get_metadata_from_date("tests/example", Some(1662480556 as f64)).unwrap();
        assert_eq!(recent_packets.len(), 1);
        assert_eq!(
            recent_packets.first().unwrap().id,
            "20170818-164847-7574883b"
        );

        let recent_packets =
            get_metadata_from_date("tests/example", Some(1662480555 as f64)).unwrap();
        assert_eq!(recent_packets.len(), 4);
    }

    #[test]
    fn can_get_packet() {
        let _packet = get_metadata_by_id("tests/example", "20180818-164043-7cdcde4b").unwrap();
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
        let digest = get_ids_digest("tests/example", None).unwrap();
        let dat = "20170818-164830-33e0ab0120170818-164847-7574883b20180220-095832-16a4bbed\
        20180818-164043-7cdcde4b";
        let expected = format!("sha256:{:x}", Sha256::new().chain_update(dat).finalize());
        assert_eq!(digest, expected);
    }

    #[test]
    fn can_get_ids_digest_with_given_alg() {
        let digest = get_ids_digest("tests/example", Some(String::from("md5"))).unwrap();
        let dat = "20170818-164830-33e0ab0120170818-164847-7574883b20180220-095832-16a4bbed\
        20180818-164043-7cdcde4b";
        let expected = format!("md5:{:x}", md5::compute(dat));
        assert_eq!(digest, expected);
    }

    #[test]
    fn can_get_ids() {
        let ids = get_ids("tests/example", None).unwrap();
        assert_eq!(ids.len(), 4);
        assert!(ids.iter().any(|e| e == "20170818-164830-33e0ab01"));
        assert!(ids.iter().any(|e| e == "20170818-164847-7574883b"));
        assert!(ids.iter().any(|e| e == "20180220-095832-16a4bbed"));
        assert!(ids.iter().any(|e| e == "20180818-164043-7cdcde4b"));
    }

    #[test]
    fn can_get_unpacked_ids() {
        let ids = get_ids("tests/example", Some(true)).unwrap();
        assert_eq!(ids.len(), 1);
        assert!(ids.iter().any(|e| e == "20170818-164847-7574883b"));
    }

    #[test]
    fn can_get_missing_ids() {
        let ids = get_missing_ids(
            "tests/example",
            &vec![
                "20180818-164043-7cdcde4b".to_string(),
                "20170818-164830-33e0ab02".to_string(),
            ],
            None,
        )
        .unwrap();
        assert_eq!(ids.len(), 1);
        assert!(ids.iter().any(|e| e == "20170818-164830-33e0ab02"));

        // check whitespace insensitivity
        let ids = get_missing_ids(
            "tests/example",
            &vec![
                "20180818-164043-7cdcde4b".to_string(),
                "20170818-164830-33e0ab02".to_string(),
            ],
            None,
        )
        .unwrap();
        assert_eq!(ids.len(), 1);
        assert!(ids.iter().any(|e| e == "20170818-164830-33e0ab02"));
    }

    #[test]
    fn can_get_missing_unpacked_ids() {
        let ids = get_missing_ids(
            "tests/example",
            &vec![
                "20170818-164847-7574883b".to_string(),
                "20170818-164830-33e0ab02".to_string(),
            ],
            Some(true),
        )
        .unwrap();
        assert_eq!(ids.len(), 1);
        assert!(ids.iter().any(|e| e == "20170818-164830-33e0ab02"));
    }

    #[test]
    fn bad_ids_raise_error() {
        let res = get_missing_ids(
            "tests/example",
            &vec![
                "20180818-164043-7cdcde4b".to_string(),
                "20170818-164830-33e0ab0".to_string(),
            ],
            None,
        )
        .map_err(|e| e.kind());
        assert_eq!(Err(io::ErrorKind::InvalidInput), res);
    }

    #[test]
    fn can_add_metadata() {
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
        let root_path = root.to_str().unwrap();
        add_metadata(root_path, data, &hash).unwrap();
        let packet = get_metadata_by_id(root_path, "20230427-150828-68772cee").unwrap();
        let expected: Value = serde_json::from_str(data).unwrap();
        assert_eq!(packet, expected);
    }

    #[test]
    fn add_metadata_is_idempotent() {
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
        let root_path = root.to_str().unwrap();
        add_metadata(root_path, data, &hash).unwrap();
        let packet = get_metadata_by_id(root_path, "20230427-150828-68772cee").unwrap();
        let expected: Value = serde_json::from_str(data).unwrap();
        assert_eq!(packet, expected);
        add_metadata(root_path, data, &hash).unwrap();
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
        let root_path = root.to_str().unwrap();
        let now = SystemTime::now();
        add_metadata(root_path, data, &hash).unwrap();
        let path = Path::new(root_path)
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
    fn cannot_put_metadata_with_missing_files() {
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
                                  "hash": "sha256:c7b512b2d14a7caae8968830760cb95980a98e18ca2c2991b87c71529e223164"
                                }
                              ],
                              "depends": [],
                              "script": [
                                "orderly.R"
                              ]
                            }"#;
        let hash = hash::hash_data(data.as_bytes(), hash::HashAlgorithm::Sha256);
        let root = get_temp_outpack_root();
        let root_path = root.to_str().unwrap();
        let res = add_metadata(root_path, data, &hash);
        assert_eq!(res.unwrap_err().to_string(),
                   "Can't import metadata for 20230427-150828-68772cee, as files missing: \n sha256:c7b512b2d14a7caae8968830760cb95980a98e18ca2c2991b87c71529e223164");
    }

    #[test]
    fn cannot_put_metadata_with_missing_dependencies() {
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
                              "depends": [{
                                "packet": "20230427-150828-68772cea",
                                "files": []
                              }],
                              "script": [
                                "orderly.R"
                              ]
                            }"#;
        let hash = hash::hash_data(data.as_bytes(), hash::HashAlgorithm::Sha256);
        let root = get_temp_outpack_root();
        let root_path = root.to_str().unwrap();
        let res = add_metadata(root_path, data, &hash);
        assert_eq!(res.unwrap_err().to_string(),
                   "Can't import metadata for 20230427-150828-68772cee, as dependencies missing: \n 20230427-150828-68772cea");
    }
}
