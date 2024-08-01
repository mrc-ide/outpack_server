#[cfg(test)]
#[macro_use]
pub mod tests {
    use crate::hash::{hash_data, Hash, HashAlgorithm};
    use crate::init::outpack_init;
    use crate::metadata::{DependencyFile, Packet, PacketDependency, PacketFile, PacketTime};
    use crate::utils::is_packet_str;
    use crate::utils::time_as_num;

    use rand::Rng;
    use std::collections::HashMap;
    use std::fs::File;
    use std::path::PathBuf;
    use std::sync::Once;
    use std::time::SystemTime;
    use tar::{Archive, Builder};
    use tempdir;

    pub fn vector_equals<T>(a: &[T], b: &[T]) -> bool
    where
        T: Eq + std::hash::Hash,
    {
        fn count<T>(items: &[T]) -> HashMap<&T, usize>
        where
            T: Eq + std::hash::Hash,
        {
            let mut cnt = HashMap::new();
            for i in items {
                *cnt.entry(i).or_insert(0) += 1
            }
            cnt
        }

        count(a) == count(b)
    }

    pub fn assert_packet_ids_eq(packets: Vec<&Packet>, ids: Vec<&str>) {
        let packet_ids: Vec<&str> = packets.iter().map(|packet| &packet.id[..]).collect();
        assert!(
            vector_equals(&packet_ids, &ids),
            "Packet ids differ to expected.\n  Packet ids are:\n  {:?}\n  Expected ids are:\n  {:?}",
            packet_ids,
            ids
        )
    }

    static INIT: Once = Once::new();

    pub fn initialize() {
        INIT.call_once(|| {
            let mut ar = Builder::new(File::create("example.tar").expect("File created"));
            ar.append_dir_all("example", "tests/example").unwrap();
            ar.finish().unwrap();
        });
    }

    pub fn get_temp_outpack_root() -> PathBuf {
        initialize();
        let tmp_dir = tempdir::TempDir::new("outpack").expect("Temp dir created");
        let mut ar = Archive::new(File::open("example.tar").unwrap());
        ar.unpack(&tmp_dir).expect("unwrapped");
        tmp_dir.into_path().join("example")
    }

    pub fn get_empty_outpack_root() -> PathBuf {
        let tmp_dir = tempdir::TempDir::new("outpack").expect("Temp dir created");

        outpack_init(
            tmp_dir.path(),
            None,
            /* use_file_store */ true,
            /* require_complete_tree */ true,
            /* default_branch */ None,
        )
        .unwrap();

        tmp_dir.into_path()
    }

    pub fn random_id() -> String {
        let now: chrono::DateTime<chrono::Utc> = SystemTime::now().into();

        let fractional = now.timestamp_subsec_nanos() as f64 / 1e9;
        let fractional = (fractional * u16::MAX as f64) as u16;

        format!(
            "{}-{:04x}{:04x}",
            now.format("%Y%m%d-%H%M%S"),
            fractional,
            rand::thread_rng().gen::<u16>()
        )
    }

    #[test]
    fn random_id_format() {
        let id = random_id();
        assert!(is_packet_str(&id), "invalid packet id {}", id);
    }

    pub struct PacketBuilder {
        packet: Packet,
    }

    /// Generate a new packet's metadata using a `PacketBuilder`.
    pub fn start_packet(name: impl Into<String>) -> PacketBuilder {
        PacketBuilder {
            packet: Packet {
                id: random_id(),
                name: name.into(),
                custom: None,
                parameters: None,
                files: Vec::new(),
                depends: Vec::new(),
                time: PacketTime {
                    start: time_as_num(SystemTime::now()),
                    end: 0.,
                },
            },
        }
    }

    impl PacketBuilder {
        pub fn add_file(
            &mut self,
            path: impl Into<String>,
            hash: impl Into<String>,
            size: usize,
        ) -> &mut PacketBuilder {
            self.packet.files.push(PacketFile {
                path: path.into(),
                hash: hash.into(),
                size,
            });
            self
        }

        pub fn add_dependency(
            &mut self,
            packet: impl Into<String>,
            files: impl Into<Vec<DependencyFile>>,
        ) -> &mut PacketBuilder {
            self.packet.depends.push(PacketDependency {
                packet: packet.into(),
                files: files.into(),
            });
            self
        }

        pub fn finish(&mut self) -> (String, String, Hash) {
            self.packet.time.end = time_as_num(SystemTime::now());
            let contents = serde_json::to_string(&self.packet).unwrap();
            let hash = hash_data(contents.as_bytes(), HashAlgorithm::Sha256);
            (self.packet.id.clone(), contents, hash)
        }
    }

    pub use lazy_static::lazy_static;
    pub use regex::Regex;
    macro_rules! assert_regex {
        ($lhs:expr, $pattern:literal) => {
            // match trick comes from the std::assert_eq implementation. It extends
            // the lifetime of any temporaries in lhs for the duration of the
            // entire block.
            match &$lhs {
                lhs => {
                    $crate::test_utils::tests::lazy_static! {
                        static ref REGEX: $crate::test_utils::tests::Regex =
                            $crate::test_utils::tests::Regex::new($pattern).unwrap();
                    }
                    assert!(
                        REGEX.is_match(lhs.as_ref()),
                        "{:?} does not match {:?}",
                        lhs,
                        $pattern
                    );
                }
            }
        };
    }
}
