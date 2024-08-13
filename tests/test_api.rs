use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Once;
use std::time::SystemTime;

use axum::body::Body;
use axum::extract::Request;
use axum::http::header::CONTENT_TYPE;
use axum::http::StatusCode;
use axum::response::Response;
use jsonschema::{Draft, JSONSchema, SchemaResolverError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tar::Archive;
use tar::Builder;
use tempdir::TempDir;
use tower::Service;
use tracing::instrument::WithSubscriber;
use tracing_capture::{CaptureLayer, SharedStorage};
use tracing_subscriber::{layer::SubscriberExt, Registry};
use url::Url;

use test_utils::{git_get_latest_commit, git_remote_branches, initialise_git_repo};

static INIT: Once = Once::new();

pub fn initialize() {
    INIT.call_once(|| {
        let mut ar = Builder::new(File::create("example.tar").expect("File created"));
        ar.append_dir_all("example", "tests/example").unwrap();
        ar.finish().unwrap();
    });
}

fn get_test_dir() -> PathBuf {
    initialize();
    let tmp_dir = TempDir::new("outpack").expect("Temp dir created");
    let mut ar = Archive::new(File::open("example.tar").unwrap());
    ar.unpack(&tmp_dir).expect("unwrapped");
    tmp_dir.into_path().join("example")
}

/// A wrapper around the Axum router to provide simpler helper functions.
///
/// These functions are designed to keep the test code clear and concise. As an effect, they do not
/// propagate errors and instead panic when they occur. This is acceptable in tests, but should not
/// be copied over to production code.
struct TestClient(axum::Router);

impl TestClient {
    fn new(root: impl Into<PathBuf>) -> TestClient {
        let api = outpack::api::api(&root.into()).unwrap();
        TestClient(api)
    }

    async fn request(&mut self, request: Request) -> Response {
        self.0.call(request).await.unwrap()
    }

    async fn get(&mut self, path: impl AsRef<str>) -> Response {
        let request = Request::get(path.as_ref()).body(Body::empty()).unwrap();
        self.request(request).await
    }

    async fn post(
        &mut self,
        path: impl AsRef<str>,
        content_type: mime::Mime,
        data: impl Into<Body>,
    ) -> Response {
        let request = Request::post(path.as_ref())
            .header(CONTENT_TYPE, content_type.as_ref())
            .body(data.into())
            .unwrap();
        self.request(request).await
    }

    async fn post_json<T: Serialize>(&mut self, path: impl AsRef<str>, data: &T) -> Response {
        self.post(
            path,
            mime::APPLICATION_JSON,
            serde_json::to_vec(data).unwrap(),
        )
        .await
    }
}

fn get_default_client() -> TestClient {
    TestClient::new(get_test_dir())
}

/// An extension trait implemented on the `Response` type for concise decoding.
///
/// Decoding errors are not propagated, and these method panic instead.
#[axum::async_trait]
trait ResponseExt {
    fn content_type(&self) -> mime::Mime;
    async fn to_bytes(self) -> axum::body::Bytes;
    async fn to_string(self) -> String;
    async fn to_json<T: for<'a> Deserialize<'a>>(self) -> T;
}

#[axum::async_trait]
impl ResponseExt for Response {
    fn content_type(&self) -> mime::Mime {
        let value = self
            .headers()
            .get(CONTENT_TYPE)
            .expect("content type header");

        value
            .to_str()
            .expect("Non-printable header value")
            .parse()
            .expect("Invalid mime type")
    }

    async fn to_bytes(self) -> axum::body::Bytes {
        let body = self.into_body();
        axum::body::to_bytes(body, usize::MAX).await.unwrap()
    }

    async fn to_string(self) -> String {
        let bytes = self.to_bytes().await;
        std::str::from_utf8(&bytes)
            .expect("Invalid utf-8 response")
            .to_owned()
    }

    async fn to_json<T: for<'a> Deserialize<'a>>(self) -> T {
        serde_json::from_slice(&self.to_bytes().await).expect("Invalid json response")
    }
}

#[test]
fn error_if_invalid_root() {
    let res = outpack::api::api(Path::new("bad-root"));
    assert_eq!(
        res.unwrap_err().to_string(),
        "Outpack root not found at 'bad-root'"
    );
}

#[tokio::test]
async fn can_get_index() {
    let mut client = get_default_client();
    let response = client.get("/").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
    validate_success("server", "root.json", &body);
}

#[tokio::test]
async fn can_get_checksum() {
    let mut client = get_default_client();

    let response = client.get("/checksum").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
    validate_success("outpack", "hash.json", &body);

    let response = client.get("/checksum?alg=md5").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
    validate_success("outpack", "hash.json", &body);

    let hash = body["data"].as_str().unwrap();
    assert!(hash.starts_with("md5:"));
}

#[tokio::test]
async fn can_list_location_metadata() {
    let mut client = get_default_client();
    let response = client.get("/metadata/list").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
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

#[tokio::test]
async fn handles_location_metadata_errors() {
    let mut client = TestClient::new("tests/bad-example");
    let response = client.get("/metadata/list").await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
    validate_error(&body, Some("missing field `packet`"));
}

#[tokio::test]
async fn can_list_metadata() {
    let mut client = get_default_client();
    let response = client.get("/packit/metadata").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
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
        entries[0].get("time").unwrap().get("start").unwrap(),
        1503074938.2232
    );
    assert_eq!(
        entries[0].get("time").unwrap().get("end").unwrap(),
        1503074938.2232
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

#[tokio::test]
async fn can_list_metadata_from_date() {
    let mut client = get_default_client();
    let response = client.get("/packit/metadata?known_since=1662480556").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
    print!("{}", body);
    validate_success("server", "list.json", &body);

    let entries = body.get("data").unwrap().as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].get("id").unwrap().as_str().unwrap(),
        "20170818-164847-7574883b"
    );
}

#[tokio::test]
async fn handles_metadata_errors() {
    let mut client = TestClient::new("tests/bad-example");
    let response = client.get("/packit/metadata").await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
    validate_error(&body, Some("missing field `name`"));
}

#[tokio::test]
async fn can_get_metadata_json() {
    let mut client = get_default_client();
    let response = client.get("/metadata/20180818-164043-7cdcde4b/json").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
    validate_success("outpack", "metadata.json", &body);
}

#[tokio::test]
async fn can_get_metadata_text() {
    let mut client = get_default_client();
    let response = client.get("/metadata/20180818-164043-7cdcde4b/text").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.content_type(), mime::TEXT_PLAIN_UTF_8);

    let expected = fs::File::open(Path::new(
        "tests/example/.outpack/metadata/20180818-164043-7cdcde4b",
    ))
    .unwrap();

    let result: Value = response.to_json().await;
    let expected: Value = serde_json::from_reader(expected).unwrap();
    assert_eq!(result, expected);
}

#[tokio::test]
async fn returns_404_if_packet_not_found() {
    let mut client = get_default_client();
    let response = client.get("/metadata/bad-id/json").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
    validate_error(&body, Some("packet with id 'bad-id' does not exist"))
}

#[tokio::test]
async fn can_get_file() {
    let mut client = get_default_client();
    let hash = "sha256:b189579a9326f585d308304bd9e03326be5d395ac71b31df359ab8bac408d248";
    let response = client.get(format!("/file/{}", hash)).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.content_type(), mime::APPLICATION_OCTET_STREAM);

    let path = Path::new("tests/example/.outpack/files/sha256/b1/")
        .join("89579a9326f585d308304bd9e03326be5d395ac71b31df359ab8bac408d248");

    let expected = fs::read(path).unwrap();

    assert_eq!(response.to_bytes().await, expected);
}

#[tokio::test]
async fn returns_404_if_file_not_found() {
    let mut client = get_default_client();
    let hash = "sha256:123456";
    let response = client.get(format!("/file/{}", hash)).await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
    validate_error(&body, Some("hash 'sha256:123456' not found"))
}

#[derive(Serialize, Deserialize)]
struct Ids {
    ids: Vec<String>,
    unpacked: bool,
}

#[tokio::test]
async fn can_get_missing_ids() {
    let mut client = get_default_client();
    let response = client
        .post_json(
            "/packets/missing",
            &Ids {
                ids: vec![
                    "20180818-164043-7cdcde4b".to_string(),
                    "20170818-164830-33e0ab01".to_string(),
                ],
                unpacked: false,
            },
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
    validate_success("server", "ids.json", &body);
    let entries = body.get("data").unwrap().as_array().unwrap();
    assert_eq!(entries.len(), 0);
}

#[tokio::test]
async fn can_get_missing_unpacked_ids() {
    let mut client = get_default_client();
    let response = client
        .post_json(
            "/packets/missing",
            &Ids {
                ids: vec![
                    "20170818-164847-7574883b".to_string(),
                    "20170818-164830-33e0ab02".to_string(),
                ],
                unpacked: true,
            },
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
    validate_success("server", "ids.json", &body);
    let entries = body.get("data").unwrap().as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries.first().unwrap().as_str(),
        Some("20170818-164830-33e0ab02")
    );
}

#[tokio::test]
async fn missing_packets_propagates_errors() {
    let mut client = get_default_client();
    let response = client
        .post_json(
            "/packets/missing",
            &Ids {
                ids: vec!["badid".to_string()],
                unpacked: true,
            },
        )
        .await;

    let body = response.to_json().await;
    validate_error(&body, Some("Invalid packet id"));
}

#[tokio::test]
async fn missing_packets_validates_request_body() {
    let mut client = get_default_client();
    let response = client
        .post("/packets/missing", mime::APPLICATION_JSON, Body::empty())
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
    validate_error(&body, Some("EOF while parsing a value at line 1 column 0"));
}

#[derive(Serialize, Deserialize)]
struct Hashes {
    hashes: Vec<String>,
}

#[tokio::test]
async fn can_get_missing_files() {
    let mut client = get_default_client();
    let response = client
        .post_json(
            "/files/missing",
            &Hashes {
                hashes: vec![
                    "sha256:b189579a9326f585d308304bd9e03326be5d395ac71b31df359ab8bac408d248"
                        .to_string(),
                    "sha256:a189579a9326f585d308304bd9e03326be5d395ac71b31df359ab8bac408d247"
                        .to_string(),
                ],
            },
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
    validate_success("server", "hashes.json", &body);
    let entries = body.get("data").unwrap().as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries.first().unwrap().as_str(),
        Some("sha256:a189579a9326f585d308304bd9e03326be5d395ac71b31df359ab8bac408d247")
    );
}

#[tokio::test]
async fn missing_files_propagates_errors() {
    let mut client = get_default_client();
    let response = client
        .post_json(
            "/files/missing",
            &Hashes {
                hashes: vec!["badhash".to_string()],
            },
        )
        .await;

    let body = response.to_json().await;
    validate_error(&body, Some("Invalid hash format 'badhash'"));
}

#[tokio::test]
async fn missing_files_validates_request_body() {
    let mut client = get_default_client();
    let response = client
        .post("/files/missing", mime::APPLICATION_JSON, Body::empty())
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
    validate_error(&body, Some("EOF while parsing a value at line 1 column 0"));
}

#[tokio::test]
async fn can_post_file() {
    let mut client = get_default_client();
    let content = "test";
    let hash = format!(
        "sha256:{:x}",
        Sha256::new().chain_update(content).finalize()
    );
    let response = client
        .post(
            format!("/file/{}", hash),
            mime::APPLICATION_OCTET_STREAM,
            content,
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
    validate_success("server", "null-response.json", &body);

    body.get("data")
        .expect("Data property present")
        .as_null()
        .expect("Null data");

    // check file now exists on server
    let get_file_response = client.get(format!("/file/{}", hash)).await;
    assert_eq!(get_file_response.status(), StatusCode::OK);
    assert_eq!(get_file_response.to_string().await, "test");
}

#[tokio::test]
async fn file_post_handles_errors() {
    let mut client = get_default_client();
    let content = "test";
    let response = client
        .post("/file/md5:bad4a54", mime::APPLICATION_OCTET_STREAM, content)
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
    validate_error(
        &body,
        Some("Expected hash 'md5:bad4a54' but found 'md5:098f6bcd4621d373cade4e832627b4f6'"),
    );
}

#[tokio::test]
async fn can_post_metadata() {
    let mut client = get_default_client();
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
        .post(format!("/packet/{}", hash), mime::TEXT_PLAIN_UTF_8, content)
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
    validate_success("server", "null-response.json", &body);

    body.get("data")
        .expect("Data property present")
        .as_null()
        .expect("Null data");

    // check packet now exists on server
    let get_metadata_response = client.get("/metadata/20230427-150828-68772cee/json").await;
    assert_eq!(get_metadata_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn catches_arbitrary_404() {
    let mut client = get_default_client();
    let response = client.get("/badurl").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
    validate_error(&body, Some("This route does not exist"));
}

#[tokio::test]
async fn exposes_metrics_endpoint() {
    let mut client = get_default_client();

    // Send at least one arbitrary request first so we don't get empty metrics.
    client.get("/").await;

    let response = client.get("/metrics").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.content_type(), "text/plain; version=0.0.4");

    assert!(response
        .to_string()
        .await
        .lines()
        .any(|line| line.starts_with("http_requests_total")));
}

#[tokio::test]
async fn generates_request_id() {
    let mut client = get_default_client();
    let response = client.get("/").await;
    assert!(!response.headers()["x-request-id"].is_empty());
}

#[tokio::test]
async fn propagates_request_id() {
    let mut client = get_default_client();
    let request = Request::get("/")
        .header("x-request-id", "foobar123")
        .body(Body::empty())
        .unwrap();
    let response = client.request(request).await;
    assert_eq!(response.headers()["x-request-id"], "foobar123");
}

#[tokio::test]
async fn request_id_is_logged() {
    use predicates::ord::eq;
    use tracing_capture::predicates::{message, name, ScanExt};

    // tracing has a pretty obscure bug when exactly one subscriber exists, but other threads are
    // calling trace macros without a subscriber. We can work around it by creating a dummy
    // subscriber in the background. We need to assign it to a variable to ensure it does not get
    // dropped and persists until the end of the test.
    // See https://github.com/tokio-rs/tracing/issues/2874
    let _dont_drop_me = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::new());

    let storage = SharedStorage::default();
    let subscriber = Registry::default().with(CaptureLayer::new(&storage));

    let f = async {
        let mut client = get_default_client();
        let request = Request::get("/")
            .header("x-request-id", "foobar123")
            .body(Body::empty())
            .unwrap();

        client.request(request).await
    };

    f.with_subscriber(subscriber).await;

    let storage = storage.lock();
    let span = storage.scan_spans().single(&name(eq("request")));
    assert!(span
        .value("request_id")
        .unwrap()
        .is_debug(&tracing::field::display("foobar123")));
    span.scan_events()
        .single(&message(eq("started processing request")));
    span.scan_events()
        .single(&message(eq("finished processing request")));
}

#[tokio::test]
async fn can_fetch_git() {
    let test_dir = get_test_dir();
    let test_git = initialise_git_repo(Some(&test_dir));
    let mut client = TestClient::new(test_git.dir.path().join("local"));

    let remote_ref = git_get_latest_commit(&test_git.remote, "HEAD");
    let initial_ref = git_get_latest_commit(&test_git.local, "refs/remotes/origin/HEAD");
    assert_ne!(
        initial_ref.message().unwrap(),
        remote_ref.message().unwrap()
    );

    let initial_branches = git_remote_branches(&test_git.local);
    assert_eq!(initial_branches.count(), 2); // HEAD and main

    let response = client
        .post("/git/fetch", mime::APPLICATION_JSON, Body::empty())
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
    validate_success("server", "null-response.json", &body);

    body.get("data")
        .expect("Data property present")
        .as_null()
        .expect("Null data");

    let post_fetch_ref = git_get_latest_commit(&test_git.local, "refs/remotes/origin/HEAD");
    assert_eq!(
        post_fetch_ref.message().unwrap(),
        remote_ref.message().unwrap()
    );

    let post_fetch_branches = git_remote_branches(&test_git.local);
    assert_eq!(post_fetch_branches.count(), 3); // HEAD, main and other
}

#[tokio::test]
async fn can_list_git_branches() {
    let test_dir = get_test_dir();
    let test_git = initialise_git_repo(Some(&test_dir));
    let now_in_seconds = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let mut client = TestClient::new(test_git.dir.path().join("local"));

    let response_fetch = client
        .post("/git/fetch", mime::APPLICATION_JSON, Body::empty())
        .await;
    assert_eq!(response_fetch.status(), StatusCode::OK);
    assert_eq!(response_fetch.content_type(), mime::APPLICATION_JSON);

    let response = client.get("/git/branches").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.content_type(), mime::APPLICATION_JSON);

    let body = response.to_json().await;
    validate_success("server", "branches.json", &body);

    let entries = body.get("data").unwrap().as_array().unwrap();

    assert_eq!(entries[0].get("name").unwrap().as_str().unwrap(), "master");
    assert_eq!(
        entries[0].get("time").unwrap().as_u64().unwrap(),
        now_in_seconds
    );
    assert_eq!(
        *entries[0].get("message").unwrap().as_array().unwrap(),
        vec!["Second commit"]
    );
    assert_eq!(entries[1].get("name").unwrap().as_str().unwrap(), "other");
    assert_eq!(
        entries[1].get("time").unwrap().as_u64().unwrap(),
        now_in_seconds
    );
    assert_eq!(
        *entries[1].get("message").unwrap().as_array().unwrap(),
        vec!["Third commit"]
    );
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

    if let Some(message) = message {
        let err = instance
            .get("errors")
            .expect("Status property present")
            .as_array()
            .unwrap()
            .first()
            .expect("First error")
            .get("detail")
            .expect("Error detail")
            .to_string();

        assert!(err.contains(message), "Error was: {}", err);
    }
}

fn assert_valid(instance: &Value, compiled: &JSONSchema) {
    let result = compiled.validate(instance);
    if let Err(errors) = result {
        for error in errors {
            println!("Validation error: {}", error);
            println!("Instance path: {}", error.instance_path);
        }
    }
    assert!(compiled.is_valid(instance));
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
        Ok(Arc::new(json_schema))
    }
}
