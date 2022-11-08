use serde::{Deserialize, Serialize};
use std::fs::File;
use std::result::Result;
use std::io::{Error};
use std::path::{Path};

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    schema_version: String,
}

pub fn read_config(root_path: &str) -> Result<Config, Error> {
    let path = Path::new(root_path)
        .join(".outpack")
        .join("config.json");

    let config_file = File::open(path)?;
    let config: Config = serde_json::from_reader(config_file)?;
    Ok(config)
}
