use anyhow::{bail, Context};
use rocket::fs::TempFile;
use rocket::serde::json::{Error, Json};
use rocket::serde::{Deserialize, Serialize};
use rocket::State;
use rocket::{catch, catchers, routes, Build, Request, Rocket};
use std::io::ErrorKind;
use std::path::Path;

use crate::config;
use crate::hash;
use crate::location;
use crate::metadata;
use crate::responses;
use crate::store;

use crate::outpack_file::OutpackFile;
use responses::{FailResponse, OutpackError, OutpackSuccess};

type OutpackResult<T> = Result<OutpackSuccess<T>, OutpackError>;

// This mostly exists to smooth over a difference with original
// version, which used Root as the object; soon we will update this to
// report actual versions back.
#[derive(Serialize, Deserialize, Debug)]
pub struct ApiRoot {
    pub schema_version: String,
}

#[catch(500)]
fn internal_error(_req: &Request) -> Json<FailResponse> {
    Json(FailResponse::from(OutpackError {
        error: String::from("UNKNOWN_ERROR"),
        detail: String::from("Something went wrong"),
        kind: Some(ErrorKind::Other),
    }))
}

#[catch(404)]
fn not_found(_req: &Request) -> Json<FailResponse> {
    Json(FailResponse::from(OutpackError {
        error: String::from("NOT_FOUND"),
        detail: String::from("This route does not exist"),
        kind: Some(ErrorKind::NotFound),
    }))
}

#[catch(400)]
fn bad_request(_req: &Request) -> Json<FailResponse> {
    Json(FailResponse::from(OutpackError {
        error: String::from("BAD_REQUEST"),
        detail: String::from(
            "The request could not be understood by the server due to malformed syntax",
        ),
        kind: Some(ErrorKind::InvalidInput),
    }))
}

#[rocket::get("/")]
fn index(_root: &State<String>) -> OutpackResult<ApiRoot> {
    Ok(ApiRoot {
        schema_version: String::from("0.1.1"),
    }
    .into())
}

#[rocket::get("/metadata/list")]
fn list_location_metadata(root: &State<String>) -> OutpackResult<Vec<location::LocationEntry>> {
    location::read_locations(root)
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

#[rocket::get("/packit/metadata?<known_since>")]
fn get_metadata(
    root: &State<String>,
    known_since: Option<f64>,
) -> OutpackResult<Vec<metadata::PackitPacket>> {
    metadata::get_packit_metadata_from_date(root, known_since)
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

#[rocket::get("/metadata/<id>/json")]
fn get_metadata_by_id(root: &State<String>, id: String) -> OutpackResult<serde_json::Value> {
    metadata::get_metadata_by_id(root, &id)
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

#[rocket::get("/metadata/<id>/text")]
fn get_metadata_raw(root: &State<String>, id: String) -> Result<String, OutpackError> {
    metadata::get_metadata_text(root, &id).map_err(OutpackError::from)
}

#[rocket::get("/file/<hash>")]
async fn get_file(root: &State<String>, hash: String) -> Result<OutpackFile, OutpackError> {
    let path = store::file_path(root, &hash);
    OutpackFile::open(hash, path?)
        .await
        .map_err(OutpackError::from)
}

#[rocket::get("/checksum?<alg>")]
async fn get_checksum(root: &State<String>, alg: Option<String>) -> OutpackResult<String> {
    metadata::get_ids_digest(root, alg)
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

#[rocket::post("/packets/missing", format = "json", data = "<ids>")]
async fn get_missing_packets(
    root: &State<String>,
    ids: Result<Json<Ids>, Error<'_>>,
) -> OutpackResult<Vec<String>> {
    let ids = ids?;
    metadata::get_missing_ids(root, &ids.ids, Some(ids.unpacked))
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

#[rocket::post("/files/missing", format = "json", data = "<hashes>")]
async fn get_missing_files(
    root: &State<String>,
    hashes: Result<Json<Hashes>, Error<'_>>,
) -> OutpackResult<Vec<String>> {
    let hashes = hashes?;
    store::get_missing_files(root, &hashes.hashes)
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

#[rocket::post("/file/<hash>", format = "binary", data = "<file>")]
async fn add_file(
    root: &State<String>,
    hash: String,
    file: TempFile<'_>,
) -> Result<OutpackSuccess<()>, OutpackError> {
    store::put_file(root, file, &hash)
        .await
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

#[rocket::post("/packet/<hash>", format = "plain", data = "<packet>")]
async fn add_packet(
    root: &State<String>,
    hash: String,
    packet: String,
) -> Result<OutpackSuccess<()>, OutpackError> {
    let hash = hash.parse::<hash::Hash>().map_err(OutpackError::from)?;
    metadata::add_metadata(root, &packet, &hash)
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
struct Ids {
    ids: Vec<String>,
    unpacked: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
struct Hashes {
    hashes: Vec<String>,
}

pub fn check_config(config: &config::Config) -> anyhow::Result<()> {
    // These two are probably always constraints for using the server:
    if !config.core.use_file_store {
        bail!("Outpack must be configured to use a file store");
    }
    if !config.core.require_complete_tree {
        bail!("Outpack must be configured to require a complete tree");
    }
    // These two we can relax over time:
    if config.core.hash_algorithm != hash::HashAlgorithm::Sha256 {
        bail!(
            "Outpack must be configured to use hash algorithm 'sha256', but you are using '{}'",
            config.core.hash_algorithm
        );
    }
    if config.core.path_archive.is_some() {
        bail!(
            "Outpack must be configured to *not* use an archive, but your path_archive is '{}'",
            config.core.path_archive.as_ref().unwrap()
        );
    }
    Ok(())
}

pub fn preflight(root_path: &str) -> anyhow::Result<()> {
    if !Path::new(&root_path).join(".outpack").exists() {
        bail!("Outpack root not found at '{}'", root_path);
    }

    let config = config::read_config(root_path)
        .with_context(|| format!("Failed to read outpack config from '{}'", root_path))?;

    check_config(&config)?;
    Ok(())
}

fn api_build(root: &str) -> Rocket<Build> {
    rocket::build()
        .manage(String::from(root))
        .register("/", catchers![internal_error, not_found, bad_request])
        .mount(
            "/",
            routes![
                index,
                list_location_metadata,
                get_metadata,
                get_metadata_by_id,
                get_metadata_raw,
                get_file,
                get_checksum,
                get_missing_packets,
                get_missing_files,
                add_file,
                add_packet
            ],
        )
}

pub fn api(root: &str) -> anyhow::Result<Rocket<Build>> {
    preflight(root)?;
    Ok(api_build(root))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(
        hash_algorithm: hash::HashAlgorithm,
        path_archive: Option<String>,
        use_file_store: bool,
        require_complete_tree: bool,
    ) -> config::Config {
        let location: Vec<config::Location> = Vec::new();
        let core = config::Core {
            hash_algorithm,
            path_archive,
            use_file_store,
            require_complete_tree,
        };
        config::Config { location, core }
    }

    #[test]
    fn can_validate_config() {
        let res = check_config(&make_config(hash::HashAlgorithm::Sha1, None, true, true));
        assert_eq!(
            res.unwrap_err().to_string(),
            "Outpack must be configured to use hash algorithm 'sha256', but you are using 'sha1'"
        );

        let res = check_config(&make_config(hash::HashAlgorithm::Sha256, None, false, true));
        assert_eq!(
            res.unwrap_err().to_string(),
            "Outpack must be configured to use a file store"
        );

        let res = check_config(&make_config(hash::HashAlgorithm::Sha256, None, true, false));
        assert_eq!(
            res.unwrap_err().to_string(),
            "Outpack must be configured to require a complete tree"
        );

        let res = check_config(&make_config(
            hash::HashAlgorithm::Sha256,
            Some(String::from("archive")),
            true,
            true,
        ));
        assert_eq!(res.unwrap_err().to_string(), "Outpack must be configured to *not* use an archive, but your path_archive is 'archive'");
    }
}
