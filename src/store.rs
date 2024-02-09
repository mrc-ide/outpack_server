use std::path::{Path, PathBuf};
use std::{fs, io};
use tempfile::tempdir_in;
use walkdir::{DirEntry, WalkDir};

use crate::hash;
use crate::upload::Upload;

pub fn file_path(root: &Path, hash: &str) -> io::Result<PathBuf> {
    let parsed: hash::Hash = hash.parse().map_err(hash::hash_error_to_io_error)?;
    Ok(root
        .join(".outpack")
        .join("files")
        .join(parsed.algorithm.to_string())
        .join(&parsed.value[..2])
        .join(&parsed.value[2..]))
}

pub fn file_exists(root: &Path, hash: &str) -> io::Result<bool> {
    let path = file_path(root, hash)?;
    Ok(fs::metadata(path).is_ok())
}

pub fn get_missing_files(root: &Path, wanted: &[String]) -> io::Result<Vec<String>> {
    wanted
        .iter()
        .filter_map(|h| match file_exists(root, h) {
            Ok(false) => Some(Ok(h.clone())),
            Ok(true) => None,
            Err(e) => Some(Err(e)),
        })
        .collect()
}

pub async fn put_file(root: &Path, file: impl Into<Upload>, hash: &str) -> io::Result<()> {
    let temp_dir = tempdir_in(root)?;
    let temp_path = temp_dir.path().join("data");

    file.into().persist(&temp_path).await?;

    hash::validate_hash_file(&temp_path, hash).map_err(hash::hash_error_to_io_error)?;
    let path = file_path(root, hash)?;
    if !file_exists(root, hash)? {
        fs::create_dir_all(path.parent().unwrap())?;
        fs::rename(temp_path, path).map(|_| ())
    } else {
        Ok(())
    }
}

pub fn enumerate_files(root: &Path) -> impl Iterator<Item = DirEntry> {
    let directory = root.join(".outpack").join("files");

    WalkDir::new(directory)
        .into_iter()
        .filter_map(|r| r.ok())
        .filter(|p| p.file_type().is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::{hash_data, HashAlgorithm};
    use crate::test_utils::tests::{get_temp_outpack_root, vector_equals};
    use std::ffi::OsString;

    #[test]
    fn can_get_path() {
        let hash = "sha256:e9aa9f2212ab";
        let res = file_path(Path::new("root"), hash).unwrap();
        assert_eq!(
            res,
            Path::new("root")
                .join(".outpack")
                .join("files")
                .join("sha256")
                .join("e9")
                .join("aa9f2212ab")
        );
    }

    #[test]
    fn path_propagates_error_on_invalid_hash() {
        let hash = "sha256";
        let res = file_path(Path::new("root"), hash);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().to_string(), "Invalid hash format 'sha256'")
    }

    #[tokio::test]
    async fn put_file_is_idempotent() {
        let root = get_temp_outpack_root();
        let data = b"Testing 123.";
        let hash = hash_data(data, HashAlgorithm::Sha256);
        let hash_str = hash.to_string();

        let res = put_file(&root, data, &hash.to_string()).await;
        let expected = file_path(&root, &hash_str).unwrap();
        let expected = expected.to_str().unwrap();
        assert!(res.is_ok());
        assert_eq!(fs::read(expected).unwrap(), data);

        let res = put_file(&root, data, &hash_str).await;
        println!("{:?}", res);
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn put_file_validates_hash_format() {
        let root = get_temp_outpack_root();
        let data = b"Testing 123.";
        let res = put_file(&root, data, "badhash").await;
        assert_eq!(
            res.unwrap_err().to_string(),
            "Invalid hash format 'badhash'"
        );
    }

    #[tokio::test]
    async fn put_file_validates_hash_match() {
        let root = get_temp_outpack_root();
        let data = b"Testing 123.";
        let res = put_file(&root, data, "md5:abcde").await;
        assert_eq!(
            res.unwrap_err().to_string(),
            "Expected hash 'md5:abcde' but found 'md5:6df8571d7b178e6fbb982ad0f5cd3bc1'"
        );
    }

    #[tokio::test]
    async fn enumerate_files_works() {
        let root = get_temp_outpack_root();
        let files: Vec<_> = enumerate_files(&root)
            .map(|entry| entry.file_name().to_owned())
            .collect();

        assert!(
            vector_equals(
                &files,
                &[OsString::from(
                    "89579a9326f585d308304bd9e03326be5d395ac71b31df359ab8bac408d248"
                )]
            ),
            "got: {:?}",
            files
        );
    }
}
