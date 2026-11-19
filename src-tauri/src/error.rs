use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorKind {
    Validation,
    NotFound,
    Conflict,
    Io,
    Db,
    Os,
    Cancelled,
    Internal,
}

/// Crosses the IPC boundary as `{kind, message, detail}`.
#[derive(Debug, Serialize, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub kind: ErrorKind,
    pub message: String,
    pub detail: Option<String>,
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::new(ErrorKind::Io, e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::new(ErrorKind::Internal, e.to_string())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        Self::new(ErrorKind::Db, e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_with_exactly_kind_message_detail() {
        let e = AppError::new(ErrorKind::Conflict, "port 80 in use").with_detail("pid 4");
        let v = serde_json::to_value(&e).unwrap();
        let obj = v.as_object().unwrap();
        let mut keys: Vec<_> = obj.keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["detail", "kind", "message"]);
        assert_eq!(v["kind"], "conflict");
        assert_eq!(v["message"], "port 80 in use");
        assert_eq!(v["detail"], "pid 4");
    }

    #[test]
    fn missing_detail_serializes_as_null() {
        let v = serde_json::to_value(AppError::new(ErrorKind::NotFound, "x")).unwrap();
        assert!(v["detail"].is_null());
    }

    #[test]
    fn io_error_converts_to_io_kind() {
        let e: AppError = std::io::Error::new(std::io::ErrorKind::NotFound, "gone").into();
        assert_eq!(e.kind, ErrorKind::Io);
    }

    #[test]
    fn sqlite_error_converts_to_db_kind() {
        let e: AppError = rusqlite::Error::QueryReturnedNoRows.into();
        assert_eq!(e.kind, ErrorKind::Db);
    }
}
