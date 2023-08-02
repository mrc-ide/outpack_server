use std::io::{ErrorKind};
use rocket::{Build, catch, catchers, Request, Rocket, routes};
use rocket::fs::TempFile;
use rocket::State;
use rocket::serde::json::{Error, Json};
use rocket::serde::{Serialize, Deserialize};

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

#[catch(400)]
fn bad_request(_req: &Request) -> Json<FailResponse> {
    Json(FailResponse::from(OutpackError {
        error: String::from("BAD_REQUEST"),
        detail: String::from("The request could not be understood by the server due to malformed syntax"),
        kind: Some(ErrorKind::InvalidInput),
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
fn get_metadata(root: &State<String>, known_since: Option<f64>) -> OutpackResult<Vec<metadata::PackitPacket>> {
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
    metadata::get_metadata_text(root, &id)
        .map_err(OutpackError::from)
}

#[rocket::get("/file/<hash>")]
async fn get_file(root: &State<String>, hash: String) -> Result<OutpackFile, OutpackError> {
    let path = store::file_path(root, &hash);
    OutpackFile::open(hash, path?).await
        .map_err(OutpackError::from)
}

#[rocket::get("/checksum?<alg>")]
async fn get_checksum(root: &State<String>, alg: Option<String>) -> OutpackResult<String> {
    metadata::get_ids_digest(root, alg)
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

#[rocket::post("/packets/missing", format = "json", data = "<ids>")]
async fn get_missing_packets(root: &State<String>, ids: Result<Json<Ids>, Error<'_>>) -> OutpackResult<Vec<String>> {
    let ids = ids?;
    metadata::get_missing_ids(root, &ids.ids, Some(ids.unpacked))
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

#[rocket::post("/files/missing", format = "json", data = "<hashes>")]
async fn get_missing_files(root: &State<String>, hashes: Result<Json<Hashes>, Error<'_>>) -> OutpackResult<Vec<String>> {
    let hashes = hashes?;
    store::get_missing_files(root, &hashes.hashes)
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

#[rocket::post("/file/<hash>", data = "<file>")]
async fn add_file(
    root: &State<String>,
    hash: String,
    file: TempFile<'_>,
) -> Result<OutpackSuccess<()>, OutpackError> {
    store::put_file(root, file, &hash).await
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

#[rocket::post("/packet/<hash>", data = "<packet>")]
async fn add_packet(
    root: &State<String>,
    hash: String,
    packet: String,
) -> Result<OutpackSuccess<()>, OutpackError> {
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

pub fn api(root: String) -> Rocket<Build> {
    rocket::build()
        .manage(root)
        .register("/", catchers![internal_error, not_found, bad_request])
        .mount("/", routes![index, list_location_metadata, get_metadata,
            get_metadata_by_id, get_metadata_raw, get_file, get_checksum, get_missing_packets,
            get_missing_files, add_file, add_packet])
}
