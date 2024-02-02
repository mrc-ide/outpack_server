use crate::metadata;
use crate::store;
use prometheus::{core::Collector, core::Desc, IntGauge, Opts, Registry};
use std::path::{Path, PathBuf};

/// A prometheus collector with metrics for the state of the repository.
///
/// The metrics are collected lazily whenever the metrics endpoint is called.
struct RepositoryCollector {
    root: PathBuf,
    metadata_total: IntGauge,
    packets_total: IntGauge,
    files_total: IntGauge,
    file_size_bytes_total: IntGauge,
    descs: Vec<Desc>,
}

impl RepositoryCollector {
    pub fn new(root: impl Into<PathBuf>) -> RepositoryCollector {
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
        RepositoryCollector {
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

impl Collector for RepositoryCollector {
    fn desc(&self) -> Vec<&prometheus::core::Desc> {
        self.descs.iter().collect()
    }

    fn collect(&self) -> Vec<prometheus::proto::MetricFamily> {
        let mut metrics = Vec::new();
        if let Err(e) = self.update() {
            log::error!("error while collecting repository metrics: {}", e);
        } else {
            metrics.extend(self.metadata_total.collect());
            metrics.extend(self.packets_total.collect());
            metrics.extend(self.files_total.collect());
            metrics.extend(self.file_size_bytes_total.collect());
        }
        metrics
    }
}

pub fn register(registry: &Registry, root: &Path) -> prometheus::Result<()> {
    registry.register(Box::new(RepositoryCollector::new(root)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::hash_data;
    use crate::hash::HashAlgorithm;
    use crate::metadata::{add_metadata, add_packet};
    use crate::store::put_file;
    use crate::test_utils::tests::{get_empty_outpack_root, start_packet};

    #[test]
    fn repository_collector_empty_repo() {
        let root = get_empty_outpack_root();
        let collector = RepositoryCollector::new(&root);

        assert_eq!(collector.metadata_total.get(), 0);
        assert_eq!(collector.packets_total.get(), 0);
        assert_eq!(collector.files_total.get(), 0);
        assert_eq!(collector.file_size_bytes_total.get(), 0);
    }

    #[tokio::test]
    async fn repository_collector_files() {
        let root = get_empty_outpack_root();
        let collector = RepositoryCollector::new(&root);

        let data1 = b"Testing 123";
        let hash1 = hash_data(data1, HashAlgorithm::Sha256).to_string();

        let data2 = b"More data";
        let hash2 = hash_data(data2, HashAlgorithm::Sha256).to_string();

        let total_size = data1.len() + data2.len();

        put_file(&root, data1, &hash1).await.unwrap();
        put_file(&root, data2, &hash2).await.unwrap();

        collector.update().unwrap();
        assert_eq!(collector.files_total.get(), 2);
        assert_eq!(collector.file_size_bytes_total.get(), total_size as i64);
    }

    #[test]
    fn repository_collector_packets() {
        let root = get_empty_outpack_root();
        let collector = RepositoryCollector::new(&root);

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
}
