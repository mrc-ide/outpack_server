use std::io::{ErrorKind};
use rocket::serde::json::{Json};
use rocket::{Build, catch, catchers, Request, Rocket, routes};
use rocket::State;

use crate::responses;
use crate::config;
use crate::location;
use crate::metadata;
use crate::store;

use responses::{FailResponse, OutpackError, OutpackSuccess};
use crate::outpack_file::OutpackFile;

type OutpackResult<T> = Result<OutpackSuccess<T>, OutpackError>;

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

#[rocket::get("/")]
fn index(root: &State<String>) -> OutpackResult<config::Root> {
    config::read_config(root)
        .map(|r| config::Root::new(r.schema_version))
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

#[rocket::get("/metadata/list")]
fn list_location_metadata(root: &State<String>) -> OutpackResult<Vec<location::LocationEntry>> {
    location::read_locations(root)
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

#[rocket::get("/packit/metadata?<known_since>")]
fn get_metadata(root: &State<String>, known_since: Option<f64>) -> OutpackResult<Vec<metadata::Packet>> {
    metadata::get_metadata_from_date(root, known_since)
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
    metadata::get_metadata_text(root, &id)
        .map_err(OutpackError::from)
}

#[rocket::get("/file/<hash>")]
pub async fn get_file(root: &State<String>, hash: String) -> Result<OutpackFile, OutpackError> {
    let path = store::file_path(root, &hash);
    OutpackFile::open(hash, path?).await
        .map_err(OutpackError::from)
}

#[rocket::get("/checksum?<alg>")]
pub async fn get_checksum(root: &State<String>, alg: Option<String>) -> OutpackResult<String> {
    metadata::get_ids_digest(root, alg)
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

#[rocket::get("/packets/missing?<ids>&<unpacked>")]
pub async fn get_missing(root: &State<String>, ids: &str, unpacked: Option<bool>) -> OutpackResult<Vec<String>> {
    metadata::get_missing_ids(root, ids, unpacked)
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

pub fn api(root: String) -> Rocket<Build> {
    rocket::build()
        .manage(root)
        .register("/", catchers![internal_error, not_found])
        .mount("/", routes![index, list_location_metadata, get_metadata,
            get_metadata_by_id, get_metadata_raw, get_file, get_checksum, get_missing])
}