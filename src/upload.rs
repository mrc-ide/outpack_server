use crate::responses::OutpackError;
use axum::body::Bytes;
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::Extension;
use futures::{Stream, TryStreamExt};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::{NamedTempFile, TempPath};
use tokio::io::AsyncWriteExt;
use tokio_util::io::StreamReader;
use tower::Layer;

#[derive(Clone)]
pub struct UploadConfig {
    directory: Arc<PathBuf>,
}

#[derive(Clone)]
pub struct UploadLayer {
    config: UploadConfig,
}

impl UploadLayer {
    /// Create an axum layer that configures the upload directory.
    pub fn new(path: impl Into<PathBuf>) -> UploadLayer {
        UploadLayer {
            config: UploadConfig {
                directory: Arc::new(path.into()),
            },
        }
    }
}

/// An axum `Extractor` that stores the request body as a temporary file.
///
/// The extractor can be configured by adding an `UploadLayer` to the axum Router. Request bodies
/// are stored in the configured directory. If no directory is configured, the system's default
/// temporary directory is used.
///
/// To aid in testing, an `Upload` object can also be created from an in-memory buffer.
///
/// This mimicks [Rocket's TempFile] type.
///
/// [Rocket's TempFile]: https://api.rocket.rs/v0.5/rocket/fs/enum.TempFile.html
pub enum Upload {
    Buffered(&'static [u8]),
    File(TempPath),
}

impl Upload {
    /// Persist the temporary file to the given path.
    ///
    /// The file is moved to the destination path. That path must be located on the same filesystem
    /// as the configured upload directory.
    pub async fn persist(self, destination: &Path) -> std::io::Result<()> {
        match self {
            Upload::Buffered(data) => {
                tokio::fs::write(destination, &data).await?;
            }
            Upload::File(path) => {
                let destination = destination.to_owned();
                tokio::task::spawn_blocking(move || path.persist(destination).unwrap()).await?
            }
        }
        Ok(())
    }
}

#[axum::async_trait]
impl<S> FromRequest<S> for Upload
where
    S: Send + Sync,
{
    type Rejection = OutpackError;

    async fn from_request(request: Request, state: &S) -> Result<Self, OutpackError> {
        let (mut parts, body) = request.into_parts();

        let config = Extension::<UploadConfig>::from_request_parts(&mut parts, state)
            .await
            .ok();

        let file = if let Some(config) = config {
            NamedTempFile::new_in(&*config.directory)?
        } else {
            NamedTempFile::new()?
        };

        stream_to_file(file.path(), body.into_data_stream()).await?;

        Ok(Upload::File(file.into_temp_path()))
    }
}

impl<S> Layer<S> for UploadLayer {
    type Service = axum::middleware::AddExtension<S, UploadConfig>;
    fn layer(&self, inner: S) -> Self::Service {
        Extension(self.config.clone()).layer(inner)
    }
}

/// Stream a request body to an on-disk file.
async fn stream_to_file<S>(path: &Path, stream: S) -> std::io::Result<()>
where
    S: Stream<Item = Result<Bytes, axum::Error>> + Unpin,
{
    let stream = stream.map_err(|err| io::Error::new(io::ErrorKind::Other, err));
    let mut reader = StreamReader::new(stream);

    let mut file = tokio::fs::File::create(path).await?;
    tokio::io::copy(&mut reader, &mut file).await?;
    file.flush().await?;

    Ok(())
}

impl From<&'static [u8]> for Upload {
    fn from(data: &'static [u8]) -> Upload {
        Upload::Buffered(data)
    }
}

impl<const N: usize> From<&'static [u8; N]> for Upload {
    fn from(data: &'static [u8; N]) -> Upload {
        Upload::Buffered(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    #[tokio::test]
    async fn upload_from_body() {
        let root = tempfile::tempdir().unwrap();
        let upload_dir = root.as_ref().join("uploads");
        std::fs::create_dir_all(&upload_dir).unwrap();

        let data: &[u8] = b"Hello, World!";
        let request = Request::get("/")
            .extension(UploadConfig {
                directory: Arc::new(upload_dir.clone()),
            })
            .body(Body::from(data))
            .unwrap();

        let upload = Upload::from_request(request, &()).await.unwrap();

        match upload {
            Upload::Buffered(..) => panic!("Unexpected variant"),
            Upload::File(ref path) => {
                assert!(path.starts_with(&upload_dir), "{:?} {:?}", path, upload_dir);
            }
        }

        let destination = root.as_ref().join("hello.txt");
        upload.persist(&destination).await.unwrap();

        let contents = tokio::fs::read(&destination).await.unwrap();
        assert_eq!(contents, data);
    }
}
