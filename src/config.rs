use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Error;
use std::path::Path;
use std::result::Result;

use crate::hash::HashAlgorithm;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Location {
    // Practically, doing anything with locations (therefore needing
    // access to the "type" and "args" fields) is going to require we
    // know how to deserialise into a union type; for example
    // https://stackoverflow.com/q/66964692
    //
    // However, we need to support the 'local' type, which takes no
    // arguments, so implement enough here to be able to write one.
    pub name: String,
    #[serde(rename = "type")]
    pub loc_type: String,
    pub args: HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Core {
    pub hash_algorithm: HashAlgorithm,
    pub path_archive: Option<String>,
    pub use_file_store: bool,
    pub require_complete_tree: bool,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Config {
    pub core: Core,
    pub location: Vec<Location>,
}

impl Config {
    pub fn new(
        path_archive: Option<String>,
        use_file_store: bool,
        require_complete_tree: bool,
    ) -> Result<Self, Error> {
        if !use_file_store && path_archive.is_none() {
            return Err(Error::new(
                std::io::ErrorKind::InvalidInput,
                "If 'path_archive' is None, then use_file_store must be true",
            ));
        }
        let hash_algorithm = HashAlgorithm::Sha256;
        let core = Core {
            hash_algorithm,
            path_archive,
            use_file_store,
            require_complete_tree,
        };
        let local = Location {
            name: String::from("local"),
            loc_type: String::from("local"),
            args: HashMap::new(),
        };
        let location: Vec<Location> = vec![local];
        Ok(Config { core, location })
    }
}

pub fn read_config(root: &Path) -> Result<Config, Error> {
    let path = root.join(".outpack").join("config.json");
    let config_file = fs::File::open(path)?;
    let config: Config = serde_json::from_reader(config_file)?;
    Ok(config)
}

pub fn write_config(config: &Config, root: &Path) -> Result<(), Error> {
    // assume .outpack exists
    let path_config = root.join(".outpack").join("config.json");
    fs::File::create(&path_config)?;
    let json = serde_json::to_string(&config)?;
    fs::write(path_config, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile;

    #[test]
    fn can_read_config() {
        let cfg = read_config(Path::new("tests/example")).unwrap();
        assert_eq!(cfg.core.hash_algorithm, HashAlgorithm::Sha256);
        assert!(cfg.core.use_file_store);
        assert!(cfg.core.require_complete_tree);
        assert!(cfg.core.path_archive.is_none());
    }

    #[test]
    fn can_write_config() {
        let cfg = Config::new(None, true, true).unwrap();
        assert_eq!(cfg.location.len(), 1);
        assert_eq!(cfg.location[0].name, "local");
        assert_eq!(cfg.location[0].loc_type, "local");
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path();
        fs::create_dir_all(path.join(".outpack")).unwrap();
        write_config(&cfg, path).unwrap();
        assert_eq!(read_config(path).unwrap(), cfg);
    }

    #[test]
    fn need_some_storage() {
        let cfg = Config::new(None, false, false);
        assert!(cfg.is_err());
        assert_eq!(
            cfg.unwrap_err().to_string(),
            "If 'path_archive' is None, then use_file_store must be true"
        );
    }
}
