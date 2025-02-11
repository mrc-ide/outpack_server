use std::io;
use std::io::ErrorKind;

use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::hash;

pub struct OutpackSuccess<T>(T);

impl<T> From<T> for OutpackSuccess<T> {
    fn from(inner: T) -> Self {
        Self(inner)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct OutpackError {
    pub error: String,
    pub detail: String,

    #[serde(skip_serializing, skip_deserializing)]
    pub kind: Option<ErrorKind>,
}

impl From<io::Error> for OutpackError {
    fn from(e: io::Error) -> Self {
        OutpackError {
            error: e.kind().to_string(),
            detail: e.to_string(),
            kind: Some(e.kind()),
        }
    }
}

impl From<hash::HashError> for OutpackError {
    fn from(e: hash::HashError) -> Self {
        OutpackError {
            // later this can be sorted out better; for now keep old
            // behaviour
            error: std::io::ErrorKind::InvalidInput.to_string(),
            detail: e.explanation,
            kind: Some(std::io::ErrorKind::InvalidInput),
        }
    }
}

impl From<JsonRejection> for OutpackError {
    fn from(e: JsonRejection) -> Self {
        OutpackError {
            error: e.to_string(),
            detail: e.body_text(),
            kind: Some(std::io::ErrorKind::InvalidInput),
        }
    }
}

impl From<git2::Error> for OutpackError {
    fn from(e: git2::Error) -> Self {
        OutpackError {
            error: e.message().to_string(),
            detail: format!("{:?}", e.code()),
            kind: Some(std::io::ErrorKind::Other),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SuccessResponse<T> {
    pub status: String,
    pub data: T,
    pub errors: Option<Vec<OutpackError>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FailResponse {
    pub status: String,
    pub data: Option<String>,
    pub errors: Option<Vec<OutpackError>>,
}

impl<T: Serialize> axum::response::IntoResponse for OutpackSuccess<T> {
    fn into_response(self) -> axum::http::Response<axum::body::Body> {
        axum::Json(SuccessResponse {
            status: String::from("success"),
            data: self.0,
            errors: None,
        })
        .into_response()
    }
}

impl axum::response::IntoResponse for OutpackError {
    fn into_response(self) -> axum::http::Response<axum::body::Body> {
        let status = match self.kind {
            Some(ErrorKind::NotFound) => StatusCode::NOT_FOUND,
            Some(ErrorKind::InvalidInput) => StatusCode::BAD_REQUEST,
            Some(ErrorKind::UnexpectedEof) => StatusCode::BAD_REQUEST,
            Some(ErrorKind::AlreadyExists) => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let body = axum::Json(FailResponse {
            status: "failure".to_owned(),
            data: None,
            errors: Some(vec![self]),
        });

        (status, body).into_response()
    }
}
