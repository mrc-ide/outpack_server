use std::any::Any;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use axum::extract::rejection::JsonRejection;
use axum::extract::{self, Query, State};
use axum::response::IntoResponse;
use axum::response::Response;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;

use crate::hash;
use crate::location;
use crate::metadata;
use crate::metrics::{
    self, register_build_info_metrics, register_process_metrics, HttpMetrics, RepositoryMetrics,
};
use crate::outpack_file::OutpackFile;
use crate::responses::{OutpackError, OutpackSuccess};
use crate::store;
use crate::upload::{Upload, UploadLayer};
use crate::{config, git};

type OutpackResult<T> = Result<OutpackSuccess<T>, OutpackError>;

// This mostly exists to smooth over a difference with original
// version, which used Root as the object; soon we will update this to
// report actual versions back.
#[derive(Serialize, Deserialize, Debug)]
pub struct ApiRoot {
    pub schema_version: String,
}

fn internal_error(_err: Box<dyn Any + Send + 'static>) -> Response {
    OutpackError {
        error: String::from("UNKNOWN_ERROR"),
        detail: String::from("Something went wrong"),
        kind: Some(ErrorKind::Other),
    }
    .into_response()
}

async fn not_found() -> OutpackError {
    OutpackError {
        error: String::from("NOT_FOUND"),
        detail: String::from("This route does not exist"),
        kind: Some(ErrorKind::NotFound),
    }
}

async fn index() -> OutpackResult<ApiRoot> {
    Ok(OutpackSuccess::from(ApiRoot {
        schema_version: String::from("0.1.1"),
    }))
}

async fn list_location_metadata(
    root: State<PathBuf>,
) -> OutpackResult<Vec<location::LocationEntry>> {
    location::read_locations(&root)
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

#[derive(Deserialize)]
struct KnownSince {
    known_since: Option<f64>,
}

async fn get_metadata_since(
    root: State<PathBuf>,
    query: Query<KnownSince>,
) -> OutpackResult<Vec<metadata::PackitPacket>> {
    metadata::get_packit_metadata_from_date(&root, query.known_since)
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

async fn get_metadata_by_id(
    root: State<PathBuf>,
    id: extract::Path<String>,
) -> OutpackResult<serde_json::Value> {
    metadata::get_metadata_by_id(&root, &id)
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

async fn get_metadata_raw(
    root: State<PathBuf>,
    id: extract::Path<String>,
) -> Result<String, OutpackError> {
    metadata::get_metadata_text(&root, &id).map_err(OutpackError::from)
}

async fn get_file(
    root: State<PathBuf>,
    hash: extract::Path<String>,
) -> Result<OutpackFile, OutpackError> {
    let path = store::file_path(&root, &hash);
    OutpackFile::open(hash.to_owned(), path?)
        .await
        .map_err(OutpackError::from)
}

#[derive(Deserialize)]
struct Algorithm {
    alg: Option<String>,
}

async fn get_checksum(root: State<PathBuf>, query: Query<Algorithm>) -> OutpackResult<String> {
    metadata::get_ids_digest(&root, query.0.alg)
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

async fn get_missing_packets(
    root: State<PathBuf>,
    ids: Result<Json<Ids>, JsonRejection>,
) -> OutpackResult<Vec<String>> {
    let ids = ids?;
    metadata::get_missing_ids(&root, &ids.ids, ids.unpacked)
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

async fn get_missing_files(
    root: State<PathBuf>,
    hashes: Result<Json<Hashes>, JsonRejection>,
) -> OutpackResult<Vec<String>> {
    let hashes = hashes?;
    store::get_missing_files(&root, &hashes.hashes)
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

async fn add_file(
    root: State<PathBuf>,
    hash: extract::Path<String>,
    file: Upload,
) -> Result<OutpackSuccess<()>, OutpackError> {
    tokio::task::spawn_blocking(move || {
        store::put_file(&root, file, &hash)
            .map_err(OutpackError::from)
            .map(OutpackSuccess::from)
    })
    .await
    .unwrap()
}

async fn add_packet(
    root: State<PathBuf>,
    hash: extract::Path<String>,
    packet: String,
) -> Result<OutpackSuccess<()>, OutpackError> {
    let hash = hash.parse::<hash::Hash>().map_err(OutpackError::from)?;
    metadata::add_packet(&root, &packet, &hash)
        .map_err(OutpackError::from)
        .map(OutpackSuccess::from)
}

async fn git_fetch(root: State<PathBuf>) -> Result<OutpackSuccess<()>, OutpackError> {
    tokio::task::spawn_blocking(move || {
        git::git_fetch(&root)
            .map_err(OutpackError::from)
            .map(OutpackSuccess::from)
    })
    .await
    .unwrap()
}

async fn git_list_branches(
    root: State<PathBuf>,
) -> Result<OutpackSuccess<git::BranchResponse>, OutpackError> {
    tokio::task::spawn_blocking(move || {
        git::git_list_branches(&root)
            .map_err(OutpackError::from)
            .map(OutpackSuccess::from)
    })
    .await
    .unwrap()
}

#[derive(Serialize, Deserialize)]
struct Ids {
    ids: Vec<String>,
    unpacked: bool,
}

#[derive(Serialize, Deserialize)]
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

pub fn preflight(root: &Path) -> anyhow::Result<()> {
    if !root.join(".outpack").exists() {
        bail!("Outpack root not found at '{}'", root.display());
    }

    let config = config::read_config(root)
        .with_context(|| format!("Failed to read outpack config from '{}'", root.display()))?;

    check_config(&config)?;
    Ok(())
}

fn make_request_span(request: &axum::extract::Request) -> tracing::span::Span {
    let request_id = String::from_utf8_lossy(request.headers()["x-request-id"].as_bytes());
    tracing::span!(
        tracing::Level::DEBUG,
        "request",
        method = tracing::field::display(request.method()),
        uri = tracing::field::display(request.uri()),
        version = tracing::field::debug(request.version()),
        request_id = tracing::field::display(request_id)
    )
}

pub fn api(root: &Path) -> anyhow::Result<Router> {
    use axum::routing::{get, post};

    let registry = prometheus::Registry::new();
    register_process_metrics(&registry).expect("process metrics registered");
    register_build_info_metrics(&registry).expect("build info metrics registered");
    RepositoryMetrics::register(&registry, root).expect("repository metrics registered");
    let http_metrics = HttpMetrics::register(&registry).expect("http metrics registered");

    preflight(root)?;

    let routes = Router::new()
        .route("/", get(index))
        .route("/metadata/list", get(list_location_metadata))
        .route("/metadata/:id/json", get(get_metadata_by_id))
        .route("/metadata/:id/text", get(get_metadata_raw))
        .route("/checksum", get(get_checksum))
        .route("/packets/missing", post(get_missing_packets))
        .route("/files/missing", post(get_missing_files))
        .route("/packit/metadata", get(get_metadata_since))
        .route("/file/:hash", get(get_file).post(add_file))
        .route("/packet/:hash", post(add_packet))
        .route("/git/fetch", post(git_fetch))
        .route("/git/branches", get(git_list_branches))
        .route("/metrics", get(|| async move { metrics::render(registry) }))
        .fallback(not_found)
        .with_state(root.to_owned());

    Ok(routes
        .layer(UploadLayer::new(root.join(".outpack").join("files")))
        .layer(TraceLayer::new_for_http().make_span_with(make_request_span))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(CatchPanicLayer::custom(internal_error))
        .layer(http_metrics.layer()))
}

pub fn serve(root: &Path, addr: &SocketAddr) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .init();

    let app = api(root)?;

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let listener = tokio::net::TcpListener::bind(addr).await?;
            tracing::info!("listening on {}", listener.local_addr().unwrap());
            axum::serve(listener, app).await?;
            Ok(())
        })
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
