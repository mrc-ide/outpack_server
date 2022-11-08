use std::io;
use rocket::serde::{Deserialize, Serialize};
use rocket::serde::json::{Json};
use rocket::http::{ContentType};
use rocket::{Build, Rocket};
use rocket::State;

#[macro_use]
extern crate rocket;

#[macro_use]
extern crate cached;

pub mod location;
pub mod config;


#[allow(clippy::result_large_err)]
#[get("/")]
fn index(root: &State<String>) -> Result<Json<config::Config>, ErrorResponder> {
    Ok(Json(config::read_config(root)?))
}

#[allow(clippy::result_large_err)]
#[get("/metadata/list")]
fn list(root: &State<String>) -> Result<Json<Vec<location::LocationEntry>>, ErrorResponder> {
    Ok(Json(location::read_locations(root)?))
}

pub fn api(root: String) -> Rocket<Build> {
    rocket::build()
        .manage(root)
        .mount("/", routes![index, list])
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
