use rocket::fs::TempFile;
use std::path::{Path, PathBuf};
use std::{fs, io};
use tempfile::tempdir_in;

use crate::hash;

pub fn file_path(root: &str, hash: &str) -> io::Result<PathBuf> {
    let parsed: hash::Hash = hash.parse().map_err(hash::hash_error_to_io_error)?;
    Ok(Path::new(root)
        .join(".outpack")
        .join("files")
        .join(parsed.algorithm.to_string())
        .join(&parsed.value[..2])
        .join(&parsed.value[2..]))
}

pub fn file_exists(root: &str, hash: &str) -> io::Result<bool> {
    let path = file_path(root, hash)?;
    Ok(fs::metadata(path).is_ok())
}

pub fn get_missing_files(root: &str, wanted: &[String]) -> io::Result<Vec<String>> {
    wanted
        .iter()
        .filter_map(|h| match file_exists(root, h) {
            Ok(false) => Some(Ok(h.clone())),
            Ok(true) => None,
            Err(e) => Some(Err(e)),
        })
        .collect()
}

pub async fn put_file(root: &str, mut file: TempFile<'_>, hash: &str) -> io::Result<()> {
    let temp_dir = tempdir_in(root)?;
    let temp_path = temp_dir.path().join("data");
    file.copy_to(&temp_path).await?;
    hash::validate_hash_file(&temp_path, hash).map_err(hash::hash_error_to_io_error)?;
    let path = file_path(root, hash)?;
    if !file_exists(root, hash)? {
        fs::create_dir_all(path.parent().unwrap())?;
        fs::rename(temp_path, path).map(|_| ())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::{hash_data, HashAlgorithm};
    use crate::test_utils::tests::get_temp_outpack_root;

    #[test]
    fn can_get_path() {
        let hash = "sha256:e9aa9f2212ab";
        let res = file_path("root", hash).unwrap();
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
        let res = file_path("root", hash);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().to_string(), "Invalid hash format 'sha256'")
    }

    #[rocket::async_test]
    async fn put_file_is_idempotent() {
        let root = get_temp_outpack_root();
        let data = "Testing 123.";
        let mut temp_file = TempFile::Buffered { content: data };
        temp_file.persist_to(root.join("input.txt")).await.unwrap();
        let hash = hash_data(data.as_bytes(), HashAlgorithm::Sha256);
        let hash_str = hash.to_string();

        let root_str = root.to_str().unwrap();
        let res = put_file(root_str, temp_file, &hash.to_string()).await;
        let expected = file_path(root_str, &hash_str).unwrap();
        let expected = expected.to_str().unwrap();
        assert!(res.is_ok());
        assert_eq!(fs::read_to_string(expected).unwrap(), data);

        let mut temp_file = TempFile::Buffered { content: data };
        temp_file.persist_to(root.join("input.txt")).await.unwrap();
        let res = put_file(root_str, temp_file, &hash_str).await;
        println!("{:?}", res);
        assert!(res.is_ok());
    }

    #[rocket::async_test]
    async fn put_file_validates_hash_format() {
        let root = get_temp_outpack_root();
        let data = "Testing 123.";
        let mut temp_file = TempFile::Buffered { content: data };
        temp_file.persist_to(root.join("input.txt")).await.unwrap();
        let root_path = root.to_str().unwrap();
        let res = put_file(root_path, temp_file, "badhash").await;
        assert_eq!(
            res.unwrap_err().to_string(),
            "Invalid hash format 'badhash'"
        );
    }

    #[rocket::async_test]
    async fn put_file_validates_hash_match() {
        let root = get_temp_outpack_root();
        let data = "Testing 123.";
        let mut temp_file = TempFile::Buffered { content: data };
        temp_file.persist_to(root.join("input.txt")).await.unwrap();
        let root_path = root.to_str().unwrap();
        let res = put_file(root_path, temp_file, "md5:abcde").await;
        assert_eq!(
            res.unwrap_err().to_string(),
            "Expected hash 'md5:abcde' but found 'md5:6df8571d7b178e6fbb982ad0f5cd3bc1'"
        );
    }
}
