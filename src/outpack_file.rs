use axum::body::Body;
use axum::response::Response;
use std::io;
use std::io::ErrorKind;
use std::path::Path;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

pub struct OutpackFile {
    hash: String,
    file: File,
    size: u64,
}

impl OutpackFile {
    pub async fn open<P: AsRef<Path>>(hash: String, path: P) -> io::Result<OutpackFile> {
        let file = File::open(path.as_ref())
            .await
            .map_err(|e| match e.kind() {
                ErrorKind::NotFound => {
                    io::Error::new(ErrorKind::NotFound, format!("hash '{}' not found", hash))
                }
                _ => e,
            })?;
        let size = file.metadata().await?.len();
        Ok(OutpackFile { hash, file, size })
    }
}

impl axum::response::IntoResponse for OutpackFile {
    fn into_response(self) -> Response {
        use axum::http::header::*;
        let stream = ReaderStream::new(self.file);
        let content_disposition = format!("attachment; filename=\"{}\"", self.hash);

        Response::builder()
            .header(CONTENT_TYPE, mime::APPLICATION_OCTET_STREAM.as_ref())
            .header(CONTENT_DISPOSITION, content_disposition)
            .header(CONTENT_LENGTH, self.size)
            .body(Body::from_stream(stream))
            .unwrap()
    }
}
