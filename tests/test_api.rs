use rocket::local::blocking::Client;
use rocket::http::{ContentType, Status};

#[test]
fn can_get_index() {
    let rocket = outpackserver::api(String::from("tests/example"));
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let response = client.get("/").dispatch();

    assert_eq!(response.status(), Status::Ok);
    assert_eq!(response.content_type(), Some(ContentType::JSON));
    assert_eq!(response.into_string(), Some("{\"schema_version\":\"0.0.1\"}".into()));
}

#[test]
fn can_get_metadata() {
    let rocket = outpackserver::api(String::from("tests/example"));
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let response = client.get("/").dispatch();

    assert_eq!(response.status(), Status::Ok);
    assert_eq!(response.content_type(), Some(ContentType::JSON));
    assert_eq!(response.into_string(), Some("{\"schema_version\":\"0.0.1\"}".into()));
}
