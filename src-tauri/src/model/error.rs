use serde::Serialize;

/// Display text here is for logs only. The UI never renders it — the frontend
/// owns all wording and localizes off `kind`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid path: {0}")]
    InvalidPath(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The scan could not be started at all. Problems *during* a scan are
    /// reported as `ScanIssue`s instead, so one unreadable directory does not
    /// discard the rest of the result.
    #[error("scan setup failed: {0}")]
    Scan(String),

    /// The frontend referred to a scan result that is no longer current. Recoverable
    /// by rescanning, which is why it carries no detail: there is nothing about the
    /// stale result worth showing.
    #[error("scan result is stale")]
    StaleScan,

    /// The token did not match the plan this process is holding: it was already
    /// executed, or a newer preview replaced it. Recoverable by previewing again.
    #[error("clean plan is stale")]
    StalePlan,
}

impl AppError {
    /// Stable machine-readable discriminant. The frontend switches on this to
    /// pick a localized message, so these strings must not change casually.
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::InvalidPath(_) => "invalidPath",
            AppError::Io(_) => "io",
            AppError::Scan(_) => "scan",
            AppError::StaleScan => "staleScan",
            AppError::StalePlan => "stalePlan",
        }
    }

    /// Untranslated technical context (a path, an OS error string) for the UI to
    /// interpolate into its own localized sentence.
    pub fn detail(&self) -> String {
        match self {
            AppError::InvalidPath(p) => p.clone(),
            AppError::Io(e) => e.to_string(),
            AppError::Scan(d) => d.clone(),
            AppError::StaleScan | AppError::StalePlan => String::new(),
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("AppError", 2)?;
        st.serialize_field("kind", self.kind())?;
        st.serialize_field("detail", &self.detail())?;
        st.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_to_kind_and_detail_without_prose() {
        let e = AppError::InvalidPath("/tmp/x".into());
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["kind"], "invalidPath");
        assert_eq!(json["detail"], "/tmp/x");
        assert_eq!(json.as_object().unwrap().len(), 2);
    }
}
