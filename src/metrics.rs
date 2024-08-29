use crate::metadata;
use crate::store;
use axum::extract::{MatchedPath, Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use futures::future::{BoxFuture, FutureExt};
use prometheus::{
    core::Collector, core::Desc, Encoder, HistogramOpts, HistogramVec, IntCounterVec, IntGauge,
    IntGaugeVec, Opts, Registry,
};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// A prometheus collector with metrics for the state of the repository.
///
/// The metrics are collected lazily whenever the metrics endpoint is called.
pub struct RepositoryMetrics {
    root: PathBuf,
    metadata_total: IntGauge,
    packets_total: IntGauge,
    files_total: IntGauge,
    file_size_bytes_total: IntGauge,
    descs: Vec<Desc>,
}

impl RepositoryMetrics {
    pub fn register(registry: &Registry, root: &Path) -> prometheus::Result<()> {
        registry.register(Box::new(RepositoryMetrics::new(root)))
    }

    pub fn new(root: impl Into<PathBuf>) -> RepositoryMetrics {
        let namespace = "outpack_server";
        let make_opts = |name: &str, help: &str| Opts::new(name, help).namespace(namespace);

        let metadata_total = IntGauge::with_opts(make_opts(
            "metadata_total",
            "Number of packet metadata in the repository",
        ))
        .unwrap();

        let packets_total = IntGauge::with_opts(make_opts(
            "packets_total",
            "Number of packets contained in the repository",
        ))
        .unwrap();

        let files_total = IntGauge::with_opts(make_opts(
            "files_total",
            "Number of files in the repository",
        ))
        .unwrap();

        let file_size_bytes_total = IntGauge::with_opts(make_opts(
            "file_size_bytes_total",
            "Total file size of the repository, in bytes",
        ))
        .unwrap();

        let mut descs = Vec::new();
        descs.extend(metadata_total.desc().into_iter().cloned());
        descs.extend(packets_total.desc().into_iter().cloned());
        descs.extend(files_total.desc().into_iter().cloned());
        descs.extend(file_size_bytes_total.desc().into_iter().cloned());
        RepositoryMetrics {
            root: root.into(),
            metadata_total,
            packets_total,
            files_total,
            file_size_bytes_total,
            descs,
        }
    }

    fn update(&self) -> anyhow::Result<()> {
        self.metadata_total
            .set(metadata::get_ids(&self.root, false)?.len() as i64);

        self.packets_total
            .set(metadata::get_ids(&self.root, true)?.len() as i64);

        let mut files_count = 0;
        let mut files_size = 0;
        for f in store::enumerate_files(&self.root) {
            files_count += 1;
            files_size += f.metadata()?.len();
        }
        self.files_total.set(files_count);
        self.file_size_bytes_total.set(files_size as i64);

        Ok(())
    }
}

impl Collector for RepositoryMetrics {
    fn desc(&self) -> Vec<&prometheus::core::Desc> {
        self.descs.iter().collect()
    }

    fn collect(&self) -> Vec<prometheus::proto::MetricFamily> {
        let mut metrics = Vec::new();
        if let Err(e) = self.update() {
            tracing::error!("error while collecting repository metrics: {}", e);
        } else {
            metrics.extend(self.metadata_total.collect());
            metrics.extend(self.packets_total.collect());
            metrics.extend(self.files_total.collect());
            metrics.extend(self.file_size_bytes_total.collect());
        }
        metrics
    }
}

#[derive(Clone)]
pub struct HttpMetrics {
    requests_total: IntCounterVec,
    requests_duration_seconds: HistogramVec,
    requests_in_flight: IntGaugeVec,
}

// The type returned by `HttpMetrics::layer()`. Unfortunately it is a might of a mouthful.
pub type HttpMetricsLayer = axum::middleware::FromFnLayer<
    fn(State<HttpMetrics>, Request, Next) -> BoxFuture<'static, Response>,
    HttpMetrics,
    (State<HttpMetrics>, Request),
>;

impl HttpMetrics {
    /// Create and register HTTP metrics.
    ///
    /// The returned object should be used to add a layer to axum router, using the `layer` method.
    pub fn register(registry: &Registry) -> prometheus::Result<HttpMetrics> {
        let metrics = HttpMetrics::new();
        registry.register(Box::new(metrics.requests_total.clone()))?;
        registry.register(Box::new(metrics.requests_duration_seconds.clone()))?;
        registry.register(Box::new(metrics.requests_in_flight.clone()))?;
        Ok(metrics)
    }

    pub fn new() -> HttpMetrics {
        HttpMetrics {
            requests_total: IntCounterVec::new(
                Opts::new("requests_total", "Total number of HTTP requests").namespace("http"),
                &["endpoint", "method", "status"],
            )
            .unwrap(),

            requests_duration_seconds: HistogramVec::new(
                HistogramOpts::new(
                    "requests_duration_seconds",
                    "HTTP request duration in seconds for all requests",
                )
                .namespace("http"),
                &["endpoint", "method", "status"],
            )
            .unwrap(),

            requests_in_flight: IntGaugeVec::new(
                Opts::new(
                    "requests_in_flight",
                    "Number of HTTP requests currently in-flight",
                )
                .namespace("http"),
                &["endpoint", "method"],
            )
            .unwrap(),
        }
    }

    /// Create a `Layer` that can be added to an Axum router to record request metrics.
    pub fn layer(&self) -> HttpMetricsLayer {
        axum::middleware::from_fn_with_state(self.clone(), |State(metrics), request, next| {
            metrics.track(request, next).boxed()
        })
    }

    /// Execute a request and record associated metrics.
    async fn track(self, req: Request, next: Next) -> Response {
        let start = Instant::now();

        // We only record metrics for paths that matched a route, using the endpoint string with
        // placeholders. If we were to use the full path we'd be at risk of blowing up the metrics'
        // cardinality by creating a set of metric for every possible request URL.
        // TODO(mrc-5003): at some point we should record unmatched paths too using a catch-all
        // metric.
        let Some(path) = req.extensions().get::<MatchedPath>().cloned() else {
            return next.run(req).await;
        };

        let method = req.method().clone();

        self.requests_in_flight
            .with_label_values(&[path.as_str(), method.as_ref()])
            .inc();

        let response = next.run(req).await;

        self.requests_in_flight
            .with_label_values(&[path.as_str(), method.as_ref()])
            .dec();

        let duration = start.elapsed().as_secs_f64();
        let status = response.status().as_u16().to_string();

        self.requests_total
            .with_label_values(&[path.as_str(), method.as_ref(), &status])
            .inc();

        self.requests_duration_seconds
            .with_label_values(&[path.as_str(), method.as_ref(), &status])
            .observe(duration);

        response
    }
}

#[cfg(target_os = "linux")]
pub fn register_process_metrics(registry: &Registry) -> prometheus::Result<()> {
    use prometheus::process_collector::ProcessCollector;
    registry.register(Box::new(ProcessCollector::for_self()))
}

#[cfg(not(target_os = "linux"))]
pub fn register_process_metrics(_registry: &Registry) -> prometheus::Result<()> {
    // The prometheus crate doesn't offer a process collector on platforms other
    // than Linux
    Ok(())
}

/// Render the metrics from a `prometheus::Registry` into an HTTP response.
pub fn render(registry: Registry) -> impl IntoResponse {
    let mut buffer = vec![];
    let encoder = prometheus::TextEncoder::new();
    let metrics = registry.gather();
    encoder.encode(&metrics, &mut buffer).unwrap();

    let headers = [(axum::http::header::CONTENT_TYPE, prometheus::TEXT_FORMAT)];
    (headers, buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::hash_data;
    use crate::hash::HashAlgorithm;
    use crate::metadata::{add_metadata, add_packet};
    use crate::store::put_file;
    use crate::test_utils::tests::{get_empty_outpack_root, start_packet};

    use axum::body::Body;
    use axum::http::StatusCode;
    use axum::Router;
    use std::sync::Arc;
    use tokio::sync::Barrier;
    use tower::Service;

    #[test]
    fn repository_collector_empty_repo() {
        let root = get_empty_outpack_root();
        let collector = RepositoryMetrics::new(root);

        assert_eq!(collector.metadata_total.get(), 0);
        assert_eq!(collector.packets_total.get(), 0);
        assert_eq!(collector.files_total.get(), 0);
        assert_eq!(collector.file_size_bytes_total.get(), 0);
    }

    #[tokio::test]
    async fn repository_collector_files() {
        let root = get_empty_outpack_root();
        let collector = RepositoryMetrics::new(&root);

        let data1 = b"Testing 123";
        let hash1 = hash_data(data1, HashAlgorithm::Sha256).to_string();

        let data2 = b"More data";
        let hash2 = hash_data(data2, HashAlgorithm::Sha256).to_string();

        let total_size = data1.len() + data2.len();

        put_file(&root, data1, &hash1).unwrap();
        put_file(&root, data2, &hash2).unwrap();

        collector.update().unwrap();
        assert_eq!(collector.files_total.get(), 2);
        assert_eq!(collector.file_size_bytes_total.get(), total_size as i64);
    }

    #[test]
    fn repository_collector_packets() {
        let root = get_empty_outpack_root();
        let collector = RepositoryMetrics::new(&root);

        // Create two different packets.
        // One of them is actually added to the repository.
        // We have the metadata for the second one, but it is missing from the repo.
        let (_, packet1, hash1) = start_packet("hello").finish();
        let (_, packet2, hash2) = start_packet("hello").finish();

        add_packet(&root, &packet1, &hash1).unwrap();
        add_metadata(&root, &packet2, &hash2).unwrap();

        collector.update().unwrap();
        assert_eq!(collector.metadata_total.get(), 2);
        assert_eq!(collector.packets_total.get(), 1);
    }

    #[tokio::test]
    async fn http_metrics() {
        use axum::routing::{get, post};
        let metrics = HttpMetrics::new();

        let mut router = Router::<()>::new()
            .route("/", get(()))
            .route("/error", get(StatusCode::BAD_REQUEST))
            .route("/upload", post(()))
            .route("/match/:id", get(()))
            .layer(metrics.layer());

        let mut send = |method: &str, path: &str| {
            let request = Request::builder()
                .method(method)
                .uri(path)
                .body(Body::empty())
                .unwrap();
            router.call(request)
        };

        let get_metric = |labels| metrics.requests_total.with_label_values(labels).get();

        send("GET", "/").await.unwrap();
        send("GET", "/").await.unwrap();
        send("GET", "/error").await.unwrap();
        send("POST", "/upload").await.unwrap();
        send("GET", "/match/1234").await.unwrap();
        send("GET", "/match/5678").await.unwrap();

        assert_eq!(get_metric(&["/", "GET", "200"]), 2);
        assert_eq!(get_metric(&["/error", "GET", "400"]), 1);
        assert_eq!(get_metric(&["/upload", "POST", "200"]), 1);
        assert_eq!(get_metric(&["/match/:id", "GET", "200"]), 2);
    }

    #[tokio::test]
    async fn http_in_flight_metric() {
        // Testing the in-flight metric needs a bit of coordination, since we need to read the
        // value while the request handlers are all executing.
        //
        // We use a pair of barriers: the first barrier is used to wait for all the request handlers
        // to be executing, and the second barrier is used to stop the barriers from exiting. In
        // between those two barriers, the main task can read the metric and get an accurate value
        // out of it.

        use axum::routing::get;
        let request_count = 4;
        let metrics = HttpMetrics::new();
        let barriers = Arc::new((
            Barrier::new(request_count + 1),
            Barrier::new(request_count + 1),
        ));

        let barriers_ = barriers.clone();
        let endpoint = || async move {
            barriers_.0.wait().await;
            barriers_.1.wait().await;
        };

        let mut router = Router::new()
            .route("/:count", get(endpoint))
            .layer(metrics.layer());

        let metric = metrics
            .requests_in_flight
            .with_label_values(&["/:count", "GET"]);

        assert_eq!(metric.get(), 0);

        let mut requests = Vec::new();
        for i in 0..request_count {
            let path = format!("/{i}");
            let f = tokio::spawn(router.call(Request::get(&path).body(Body::empty()).unwrap()));
            requests.push(f);
        }

        barriers.0.wait().await;
        assert_eq!(metric.get(), request_count as i64);
        barriers.1.wait().await;

        for r in requests {
            r.await.unwrap().unwrap();
        }

        assert_eq!(metric.get(), 0);
    }
}
