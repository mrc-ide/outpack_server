use cached::instant::SystemTime;
use lazy_static::lazy_static;
use regex::Regex;
use std::ffi::OsString;
use std::path::Path;
use std::time::UNIX_EPOCH;
use std::{fs, io, io::Write};

lazy_static! {
    static ref ID_REG: Regex = Regex::new(r"^([0-9]{8}-[0-9]{6}-[[:xdigit:]]{8})$").unwrap();
}

pub fn is_packet(name: &OsString) -> bool {
    name.to_str().is_some_and(is_packet_str)
}

pub fn is_packet_str(name: &str) -> bool {
    ID_REG.is_match(name)
}

pub fn time_as_num(time: SystemTime) -> f64 {
    (time.duration_since(UNIX_EPOCH).unwrap().as_millis() as f64) / 1000.0
}

/// Write a byte slice to disk.
///
/// Succeeds if the file already exists with identical contents.
/// On the other hand, an AlreadyExists error is returned if the file exists with different contents.
pub fn write_file_idempotent(path: &Path, contents: &[u8]) -> io::Result<()> {
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut f) => {
            f.write_all(contents)?;
        }
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            if fs::read(path)? != contents {
                return Err(err);
            }
        }
        Err(err) => return Err(err),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn can_detect_packet_id() {
        assert!(!is_packet(&OsString::from("1234")));
        assert!(is_packet(&OsString::from("20170818-164830-33e0ab01")));
        assert!(is_packet(&OsString::from("20180818-164847-54699abf")))
    }

    #[test]
    fn converts_time_to_seconds() {
        let epoch_ms = 1688033668123;
        let time = UNIX_EPOCH + Duration::from_millis(epoch_ms);
        let res = time_as_num(time);
        assert_eq!(res, 1688033668.123);
    }

    #[test]
    fn write_file_idempotent_writes_to_disk() {
        let base = tempdir().unwrap();
        let path = base.path().join("hello.txt");

        write_file_idempotent(&path, b"Hello").unwrap();

        let contents = fs::read(path).unwrap();
        assert_eq!(contents, b"Hello");
    }

    #[test]
    fn write_file_idempotent_succeeds_if_content_is_identical() {
        let base = tempdir().unwrap();
        let path = base.path().join("hello.txt");

        fs::write(&path, b"Hello").unwrap();

        write_file_idempotent(&path, b"Hello").unwrap();

        let contents = fs::read(path).unwrap();
        assert_eq!(contents, b"Hello");
    }

    #[test]
    fn write_file_idempotent_succeeds_if_content_is_different() {
        let base = tempdir().unwrap();
        let path = base.path().join("hello.txt");

        fs::write(&path, b"Hello").unwrap();

        let error = write_file_idempotent(&path, b"World").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);

        let contents = fs::read(path).unwrap();
        assert_eq!(contents, b"Hello");
    }
}
