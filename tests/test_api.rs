use jsonschema::{Draft, JSONSchema, SchemaResolverError};
use rocket::http::{ContentType, Status};
use rocket::local::blocking::Client;
use rocket::serde::{Deserialize, Serialize};
use rocket::{Build, Rocket};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use tar::Archive;
use tar::Builder;
use tempdir::TempDir;
use url::Url;

use std::sync::Once;

static INIT: Once = Once::new();

pub fn initialize() {
    INIT.call_once(|| {
        let mut ar = Builder::new(File::create("example.tar").expect("File created"));
        ar.append_dir_all("example", "tests/example").unwrap();
        ar.finish().unwrap();
    });
}

fn get_test_dir() -> String {
    initialize();
    let tmp_dir = TempDir::new("outpack").expect("Temp dir created");
    let mut ar = Archive::new(File::open("example.tar").unwrap());
    ar.unpack(&tmp_dir).expect("unwrapped");
    let root = Path::new(&tmp_dir.into_path()).join("example");
    String::from(root.to_str().expect("Test root"))
}

fn get_test_rocket() -> Rocket<Build> {
    let root = get_test_dir();
    outpack::api::api(&root).unwrap()
}

#[test]
fn can_get_index() {
    let rocket = get_test_rocket();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let response = client.get("/").dispatch();

    assert_eq!(response.status(), Status::Ok);
    assert_eq!(response.content_type(), Some(ContentType::JSON));

    let body = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    validate_success("server", "root.json", &body);
}

#[test]
fn error_if_cant_get_index() {
    let res = outpack::api::api("bad-root");
    assert_eq!(
        res.unwrap_err().to_string(),
        String::from("Outpack root not found at 'bad-root'")
    );
}

#[test]
fn can_get_checksum() {
    let rocket = get_test_rocket();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let response = client.get("/checksum").dispatch();

    assert_eq!(response.status(), Status::Ok);
    assert_eq!(response.content_type(), Some(ContentType::JSON));

    let body = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    validate_success("outpack", "hash.json", &body);

    let response = client.get("/checksum?alg=md5").dispatch();

    assert_eq!(response.status(), Status::Ok);
    assert_eq!(response.content_type(), Some(ContentType::JSON));

    let response_string = &response.into_string().unwrap();
    let body = serde_json::from_str(response_string).unwrap();
    validate_success("outpack", "hash.json", &body);
    assert!(response_string.contains("md5"))
}

#[test]
fn can_list_location_metadata() {
    let rocket = get_test_rocket();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let response = client.get("/metadata/list").dispatch();

    assert_eq!(response.status(), Status::Ok);
    assert_eq!(response.content_type(), Some(ContentType::JSON));

    let body: Value = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    print!("{}", body);
    validate_success("server", "locations.json", &body);

    let entries = body.get("data").unwrap().as_array().unwrap();
    assert_eq!(entries.len(), 4);

    assert_eq!(
        entries[0].get("packet").unwrap().as_str().unwrap(),
        "20170818-164847-7574883b"
    );
    assert_eq!(
        entries[0].get("time").unwrap().as_f64().unwrap(),
        1662480556.1778
    );
    assert_eq!(
        entries[0].get("hash").unwrap().as_str().unwrap(),
        "sha256:af3c863f96898c6c88cee4daa1a6d6cfb756025e70059f5ea4dbe4d9cc5e0e36"
    );

    assert_eq!(
        entries[1].get("packet").unwrap().as_str().unwrap(),
        "20170818-164830-33e0ab01"
    );
    assert_eq!(
        entries[2].get("packet").unwrap().as_str().unwrap(),
        "20180220-095832-16a4bbed"
    );
    assert_eq!(
        entries[3].get("packet").unwrap().as_str().unwrap(),
        "20180818-164043-7cdcde4b"
    );
}

#[test]
fn handles_location_metadata_errors() {
    let rocket = outpack::api::api("tests/bad-example").unwrap();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let response = client.get("/metadata/list").dispatch();
    assert_eq!(response.status(), Status::InternalServerError);
    assert_eq!(response.content_type(), Some(ContentType::JSON));

    let body = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    validate_error(&body, Some("missing field `packet`"));
}

#[test]
fn can_list_metadata() {
    let rocket = get_test_rocket();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let response = client.get("/packit/metadata").dispatch();

    assert_eq!(response.status(), Status::Ok);
    assert_eq!(response.content_type(), Some(ContentType::JSON));

    let body: Value = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    print!("{}", body);
    validate_success("server", "list.json", &body);

    let entries = body.get("data").unwrap().as_array().unwrap();
    assert_eq!(entries.len(), 4);

    assert_eq!(
        entries[0].get("id").unwrap().as_str().unwrap(),
        "20170818-164830-33e0ab01"
    );
    assert_eq!(
        entries[0]
            .get("parameters")
            .unwrap()
            .get("disease")
            .unwrap(),
        "YF"
    );
    assert_eq!(
        entries[0]
            .get("parameters")
            .unwrap()
            .as_object()
            .unwrap()
            .get("disease")
            .unwrap()
            .as_str()
            .unwrap(),
        "YF"
    );
    assert_eq!(
        entries[0]
            .get("custom")
            .unwrap()
            .as_object()
            .unwrap()
            .get("orderly")
            .unwrap()
            .as_object()
            .unwrap()
            .get("displayname")
            .unwrap()
            .as_str()
            .unwrap(),
        "Modified Update"
    );

    assert_eq!(
        entries[1].get("id").unwrap().as_str().unwrap(),
        "20170818-164847-7574883b"
    );
    assert_eq!(
        entries[2].get("id").unwrap().as_str().unwrap(),
        "20180220-095832-16a4bbed"
    );
    assert_eq!(
        entries[3].get("id").unwrap().as_str().unwrap(),
        "20180818-164043-7cdcde4b"
    );
}

#[test]
fn can_list_metadata_from_date() {
    let rocket = get_test_rocket();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let response = client
        .get("/packit/metadata?known_since=1662480556")
        .dispatch();

    assert_eq!(response.status(), Status::Ok);
    assert_eq!(response.content_type(), Some(ContentType::JSON));

    let body: Value = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    print!("{}", body);
    validate_success("server", "list.json", &body);

    let entries = body.get("data").unwrap().as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].get("id").unwrap().as_str().unwrap(),
        "20170818-164847-7574883b"
    );
}

#[test]
fn handles_metadata_errors() {
    let rocket = outpack::api::api("tests/bad-example").unwrap();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let response = client.get("/packit/metadata").dispatch();
    assert_eq!(response.status(), Status::InternalServerError);
    assert_eq!(response.content_type(), Some(ContentType::JSON));

    let body = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    validate_error(&body, Some("missing field `name`"));
}

#[test]
fn can_get_metadata_json() {
    let rocket = get_test_rocket();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let response = client
        .get("/metadata/20180818-164043-7cdcde4b/json")
        .dispatch();

    assert_eq!(response.status(), Status::Ok);
    assert_eq!(response.content_type(), Some(ContentType::JSON));

    let body: Value = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    validate_success("outpack", "metadata.json", &body);
}

#[test]
fn can_get_metadata_text() {
    let rocket = get_test_rocket();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let response = client
        .get("/metadata/20180818-164043-7cdcde4b/text")
        .dispatch();

    assert_eq!(response.status(), Status::Ok);
    assert_eq!(response.content_type(), Some(ContentType::Text));

    let expected = fs::File::open(Path::new(
        "tests/example/.outpack/metadata/20180818-164043-7cdcde4b",
    ))
    .unwrap();
    let result: Value = serde_json::from_str(&response.into_string().unwrap()[..]).unwrap();
    let expected: Value = serde_json::from_reader(expected).unwrap();
    assert_eq!(result, expected);
}

#[test]
fn returns_404_if_packet_not_found() {
    let rocket = get_test_rocket();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let response = client.get("/metadata/bad-id/json").dispatch();

    assert_eq!(response.status(), Status::NotFound);
    assert_eq!(response.content_type(), Some(ContentType::JSON));

    let body = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    validate_error(&body, Some("packet with id 'bad-id' does not exist"))
}

#[test]
fn can_get_file() {
    let rocket = get_test_rocket();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let hash = "sha256:b189579a9326f585d308304bd9e03326be5d395ac71b31df359ab8bac408d248";
    let response = client.get(format!("/file/{}", hash)).dispatch();

    assert_eq!(response.status(), Status::Ok);
    assert_eq!(response.content_type(), Some(ContentType::Binary));

    let path = Path::new("tests/example/.outpack/files/sha256/b1/")
        .join("89579a9326f585d308304bd9e03326be5d395ac71b31df359ab8bac408d248");
    let mut file = fs::File::open(&path).unwrap();
    let metadata = fs::metadata(&path).unwrap();
    let mut buffer = vec![0; metadata.len() as usize];

    file.read(&mut buffer).unwrap();

    assert_eq!(response.into_bytes().unwrap(), buffer);
}

#[test]
fn returns_404_if_file_not_found() {
    let rocket = get_test_rocket();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let hash = "sha256:123456";
    let response = client.get(format!("/file/{}", hash)).dispatch();

    assert_eq!(response.status(), Status::NotFound);
    assert_eq!(response.content_type(), Some(ContentType::JSON));

    let body = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    validate_error(&body, Some("hash 'sha256:123456' not found"))
}

#[derive(Serialize, Deserialize)]
struct Ids {
    ids: Vec<String>,
    unpacked: bool,
}

#[test]
fn can_get_missing_ids() {
    let rocket = get_test_rocket();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let response = client
        .post("/packets/missing")
        .json(&Ids {
            ids: vec![
                "20180818-164043-7cdcde4b".to_string(),
                "20170818-164830-33e0ab01".to_string(),
            ],
            unpacked: false,
        })
        .dispatch();

    assert_eq!(response.status(), Status::Ok);
    assert_eq!(response.content_type(), Some(ContentType::JSON));

    let body: Value = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    validate_success("server", "ids.json", &body);
    let entries = body.get("data").unwrap().as_array().unwrap();
    assert_eq!(entries.len(), 0);
}

#[test]
fn can_get_missing_unpacked_ids() {
    let rocket = get_test_rocket();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let response = client
        .post("/packets/missing")
        .json(&Ids {
            ids: vec![
                "20170818-164847-7574883b".to_string(),
                "20170818-164830-33e0ab02".to_string(),
            ],
            unpacked: true,
        })
        .dispatch();
    assert_eq!(response.status(), Status::Ok);
    assert_eq!(response.content_type(), Some(ContentType::JSON));

    let body: Value = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    validate_success("server", "ids.json", &body);
    let entries = body.get("data").unwrap().as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries.first().unwrap().as_str(),
        Some("20170818-164830-33e0ab02")
    );
}

#[test]
fn missing_packets_propagates_errors() {
    let rocket = get_test_rocket();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let response = client
        .post("/packets/missing")
        .json(&Ids {
            ids: vec!["badid".to_string()],
            unpacked: true,
        })
        .dispatch();

    let body: Value = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    validate_error(&body, Some("Invalid packet id"));
}

#[test]
fn missing_packets_validates_request_body() {
    let rocket = get_test_rocket();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let response = client
        .post("/packets/missing")
        .header(ContentType::JSON)
        .dispatch();

    assert_eq!(response.status(), Status::BadRequest);
    assert_eq!(response.content_type(), Some(ContentType::JSON));

    let body: Value = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    validate_error(&body, Some("EOF while parsing a value at line 1 column 0"));
}

#[derive(Serialize, Deserialize)]
struct Hashes {
    hashes: Vec<String>,
}

#[test]
fn can_get_missing_files() {
    let rocket = get_test_rocket();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let response = client
        .post("/files/missing")
        .json(&Hashes {
            hashes: vec![
                "sha256:b189579a9326f585d308304bd9e03326be5d395ac71b31df359ab8bac408d248"
                    .to_string(),
                "sha256:a189579a9326f585d308304bd9e03326be5d395ac71b31df359ab8bac408d247"
                    .to_string(),
            ],
        })
        .dispatch();

    assert_eq!(response.status(), Status::Ok);
    assert_eq!(response.content_type(), Some(ContentType::JSON));

    let body: Value = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    validate_success("server", "hashes.json", &body);
    let entries = body.get("data").unwrap().as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries.first().unwrap().as_str(),
        Some("sha256:a189579a9326f585d308304bd9e03326be5d395ac71b31df359ab8bac408d247")
    );
}

#[test]
fn missing_files_propagates_errors() {
    let rocket = get_test_rocket();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let response = client
        .post("/files/missing")
        .json(&Hashes {
            hashes: vec!["badhash".to_string()],
        })
        .dispatch();

    let body: Value = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    validate_error(&body, Some("Invalid hash format 'badhash'"));
}

#[test]
fn missing_files_validates_request_body() {
    let rocket = get_test_rocket();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let response = client
        .post("/files/missing")
        .header(ContentType::JSON)
        .dispatch();

    assert_eq!(response.status(), Status::BadRequest);
    assert_eq!(response.content_type(), Some(ContentType::JSON));

    let body: Value = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    validate_error(&body, Some("EOF while parsing a value at line 1 column 0"));
}

#[test]
fn can_post_file() {
    let root = get_test_dir();
    let rocket = outpack::api::api(&root).unwrap();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let content = "test";
    let hash = format!(
        "sha256:{:x}",
        Sha256::new().chain_update(content).finalize()
    );
    let response = client
        .post(format!("/file/{}", hash))
        .body(content)
        .header(ContentType::Binary)
        .dispatch();

    assert_eq!(response.status(), Status::Ok);
    assert_eq!(response.content_type(), Some(ContentType::JSON));

    let body = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    validate_success("server", "null-response.json", &body);

    body.get("data")
        .expect("Data property present")
        .as_null()
        .expect("Null data");

    // check file now exists on server
    let get_file_response = client.get(format!("/file/{}", hash)).dispatch();
    assert_eq!(get_file_response.status(), Status::Ok);
    assert_eq!(get_file_response.into_string().unwrap(), "test");
}

#[test]
fn file_post_handles_errors() {
    let root = get_test_dir();
    let rocket = outpack::api::api(&root).unwrap();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let content = "test";
    let response = client
        .post(format!("/file/md5:bad4a54"))
        .body(content)
        .header(ContentType::Binary)
        .dispatch();

    assert_eq!(response.status(), Status::BadRequest);
    assert_eq!(response.content_type(), Some(ContentType::JSON));

    let body = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    validate_error(
        &body,
        Some("Expected hash 'md5:bad4a54' but found 'md5:098f6bcd4621d373cade4e832627b4f6'"),
    );
}

#[test]
fn can_post_metadata() {
    let root = get_test_dir();
    let rocket = outpack::api::api(&root).unwrap();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let content = r#"{
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
    let hash = format!(
        "sha256:{:x}",
        Sha256::new().chain_update(content).finalize()
    );
    let response = client
        .post(format!("/packet/{}", hash))
        .body(content)
        .header(ContentType::Text)
        .dispatch();

    assert_eq!(response.status(), Status::Ok);
    assert_eq!(response.content_type(), Some(ContentType::JSON));

    let body = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    validate_success("server", "null-response.json", &body);

    body.get("data")
        .expect("Data property present")
        .as_null()
        .expect("Null data");

    // check packet now exists on server
    let get_metadata_response = client
        .get("/metadata/20230427-150828-68772cee/json")
        .dispatch();
    assert_eq!(get_metadata_response.status(), Status::Ok);
}

#[test]
fn catches_arbitrary_404() {
    let rocket = get_test_rocket();
    let client = Client::tracked(rocket).expect("valid rocket instance");
    let response = client.get("/badurl").dispatch();

    assert_eq!(response.status(), Status::NotFound);
    assert_eq!(response.content_type(), Some(ContentType::JSON));

    let body = serde_json::from_str(&response.into_string().unwrap()).unwrap();
    validate_error(&body, Some("This route does not exist"));
}

fn validate_success(schema_group: &str, schema_name: &str, instance: &Value) {
    let compiled_schema = get_schema("server", "response-success.json");
    assert_valid(instance, &compiled_schema);
    let status = instance.get("status").expect("Status property present");
    assert_eq!(status, "success");

    let data = instance.get("data").expect("Data property present");
    let compiled_schema = get_schema(schema_group, schema_name);
    assert_valid(data, &compiled_schema);
}

fn validate_error(instance: &Value, message: Option<&str>) {
    let compiled_schema = get_schema("server", "response-failure.json");
    assert_valid(instance, &compiled_schema);
    let status = instance.get("status").expect("Status property present");
    assert_eq!(status, "failure");

    if message.is_some() {
        let err = instance
            .get("errors")
            .expect("Status property present")
            .as_array()
            .unwrap()
            .get(0)
            .expect("First error")
            .get("detail")
            .expect("Error detail")
            .to_string();

        assert!(err.contains(message.unwrap()), "Error was: {}", err);
    }
}

fn assert_valid(instance: &Value, compiled: &JSONSchema) {
    let result = compiled.validate(&instance);
    if let Err(errors) = result {
        for error in errors {
            println!("Validation error: {}", error);
            println!("Instance path: {}", error.instance_path);
        }
    }
    assert!(compiled.is_valid(&instance));
}

fn get_schema(schema_group: &str, schema_name: &str) -> JSONSchema {
    let schema_path = Path::new("schema").join(schema_group).join(schema_name);
    let schema_as_string = fs::read_to_string(schema_path).expect("Schema file");

    let json_schema = serde_json::from_str(&schema_as_string).expect("Schema is valid json");

    JSONSchema::options()
        .with_draft(Draft::Draft7)
        .with_resolver(LocalSchemaResolver {
            base: String::from(schema_group),
        })
        .compile(&json_schema)
        .expect("A valid schema")
}

struct LocalSchemaResolver {
    base: String,
}

impl jsonschema::SchemaResolver for LocalSchemaResolver {
    fn resolve(
        &self,
        _root_schema: &Value,
        _url: &Url,
        original_reference: &str,
    ) -> Result<Arc<Value>, SchemaResolverError> {
        let schema_path = Path::new("schema")
            .join(&self.base)
            .join(original_reference);
        let schema_as_string = fs::read_to_string(schema_path).expect("Schema file");
        let json_schema = serde_json::from_str(&schema_as_string).expect("Schema is valid json");
        return Ok(Arc::new(json_schema));
    }
}
