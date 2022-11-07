use std::io;
use rocket::serde::{Deserialize, Serialize};
use rocket::serde::json::{Json};
use rocket::http::{ContentType};
use rocket::{Build, Rocket};
use rocket::State;

#[macro_use]
extern crate rocket;

pub mod location;
pub mod config;


#[get("/")]
fn index(root: &State<String>) -> Result<Json<config::Config>, ErrorResponder> {
    return Ok(Json(config::read_config(root)?));
}

#[get("/metadata/list")]
fn list(root: &State<String>) -> Result<Json<Vec<location::LocationEntry>>, ErrorResponder> {
    return Ok(Json(location::read_locations(root)?));
}

pub fn api(root: String) -> Rocket<Build> {
    return rocket::build().manage(root).mount("/", routes![index, list]);
}

#[derive(Responder)]
#[response(status = 500, content_type = "json")]
struct ErrorResponder {
    inner: Json<ApiError>,
    header: ContentType,
}

#[derive(Serialize, Deserialize, Debug)]
struct ApiError {
    message: String,
}

impl From<io::Error> for ErrorResponder {
    fn from(e: io::Error) -> Self {
        ErrorResponder { inner: Json(ApiError { message: e.to_string() }), header: ContentType::JSON }
    }
}
