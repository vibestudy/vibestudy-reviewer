use crate::error::ApiError;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::time::{timeout, Duration};

const CLONE_TIMEOUT_SECS: u64 = 300;

pub struct ClonedRepo {
    pub path: PathBuf,
    _temp_dir: Option<TempDir>,
}

impl ClonedRepo {
    pub async fn from_url(url: &str) -> Result<Self, ApiError> {
        let temp_dir = TempDir::new()
            .map_err(|e| ApiError::GitError(format!("Failed to create temp dir: {}", e)))?;

        let path = temp_dir.path().to_path_buf();
        let url = url.to_string();

        let clone_result = timeout(
            Duration::from_secs(CLONE_TIMEOUT_SECS),
            tokio::task::spawn_blocking(move || {
                let mut builder = git2::build::RepoBuilder::new();
                let mut fetch_opts = git2::FetchOptions::new();
                fetch_opts.depth(1);
                builder.fetch_options(fetch_opts);
                builder.clone(&url, &path)
            }),
        )
        .await;

        match clone_result {
            Ok(Ok(Ok(_))) => Ok(Self {
                path: temp_dir.path().to_path_buf(),
                _temp_dir: Some(temp_dir),
            }),
            Ok(Ok(Err(e))) => Err(ApiError::GitError(format!("Clone failed: {}", e))),
            Ok(Err(e)) => Err(ApiError::GitError(format!("Clone task failed: {}", e))),
            Err(_) => Err(ApiError::GitError("Clone timed out".to_string())),
        }
    }

    pub fn from_local(path: PathBuf) -> Result<Self, ApiError> {
        if !path.exists() {
            return Err(ApiError::GitError(format!(
                "Path does not exist: {:?}",
                path
            )));
        }
        Ok(Self {
            path,
            _temp_dir: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_local_nonexistent() {
        let result = ClonedRepo::from_local(PathBuf::from("/nonexistent/path"));
        assert!(result.is_err());
    }

    #[test]
    fn test_from_local_exists() {
        let result = ClonedRepo::from_local(PathBuf::from("."));
        assert!(result.is_ok());
    }
}
