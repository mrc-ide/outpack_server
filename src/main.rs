use std::io;
use rocket::serde::{Deserialize, Serialize};
use rocket::serde::json::{Json};
use rocket::http::{ContentType};

#[macro_use]
extern crate rocket;

mod location;
mod config;

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

#[get("/")]
fn index() -> Result<Json<config::Config>, ErrorResponder> {
    return Ok(Json(config::read_config("montagu-reports")?));
}

#[get("/metadata/list")]
fn list() -> Result<Json<Vec<location::LocationEntry>>, ErrorResponder> {
    return Ok(Json(location::read_locations("montagu-reports")?));
}

#[rocket::main]
#[allow(unused_must_use)]
async fn main() {
    rocket::build().mount("/", routes![index, list]).launch().await;
}
