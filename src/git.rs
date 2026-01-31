use crate::error::ApiError;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::time::{timeout, Duration};

const CLONE_TIMEOUT_SECS: u64 = 300;
const VALIDATION_TIMEOUT_SECS: u64 = 10;

async fn validate_github_repo(url: &str) -> Result<(), ApiError> {
    let github_api_url = if let Some(captures) = extract_github_info(url) {
        format!("https://api.github.com/repos/{}/{}", captures.0, captures.1)
    } else {
        return Ok(());
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(VALIDATION_TIMEOUT_SECS))
        .build()
        .map_err(|e| ApiError::GitError(format!("Failed to create HTTP client: {}", e)))?;

    let response = client
        .head(&github_api_url)
        .header("User-Agent", "code-review-api")
        .send()
        .await
        .map_err(|e| ApiError::GitError(format!("Failed to validate repository: {}", e)))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(ApiError::GitError(format!(
            "Repository not found: {}",
            url
        )));
    }

    if !response.status().is_success() && response.status() != reqwest::StatusCode::FORBIDDEN {
        return Err(ApiError::GitError(format!(
            "Failed to validate repository (status {}): {}",
            response.status(),
            url
        )));
    }

    Ok(())
}

pub fn extract_github_info(url: &str) -> Option<(String, String)> {
    let url = url.trim_end_matches(".git");
    
    if url.contains("github.com") {
        let parts: Vec<&str> = url.split('/').collect();
        if parts.len() >= 2 {
            let repo = parts[parts.len() - 1];
            let owner = parts[parts.len() - 2];
            if !owner.is_empty() && !repo.is_empty() && owner != "github.com" {
                return Some((owner.to_string(), repo.to_string()));
            }
        }
    }
    
    None
}

pub struct ClonedRepo {
    pub path: PathBuf,
    _temp_dir: Option<TempDir>,
}

impl ClonedRepo {
    pub async fn from_url(url: &str) -> Result<Self, ApiError> {
        validate_github_repo(url).await?;

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

    /// Get the short (7-char) HEAD commit hash
    pub fn head_commit_short(&self) -> Option<String> {
        let repo = git2::Repository::open(&self.path).ok()?;
        let head = repo.head().ok()?;
        let commit = head.peel_to_commit().ok()?;
        let full_hash = commit.id().to_string();
        Some(full_hash[..7.min(full_hash.len())].to_string())
    }

    /// Generate a cache key for this repo: "owner/repo:branch:commit"
    pub fn cache_key(&self, repo_url: &str, branch: Option<&str>) -> Option<String> {
        let (owner, repo) = extract_github_info(repo_url)?;
        let commit = self.head_commit_short()?;
        let branch = branch.unwrap_or("main");
        Some(format!("{}:{}:{}:{}", owner, repo, branch, commit))
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

    #[test]
    fn test_extract_github_info_junho_io_v2() {
        let result = extract_github_info("https://github.com/junhoyeo/junho.io-v2");
        assert_eq!(result, Some(("junhoyeo".to_string(), "junho.io-v2".to_string())));
    }

    #[test]
    fn test_extract_github_info_tokscale() {
        let result = extract_github_info("https://github.com/junhoyeo/tokscale");
        assert_eq!(result, Some(("junhoyeo".to_string(), "tokscale".to_string())));
    }

    #[test]
    fn test_extract_github_info_with_git_suffix() {
        let result = extract_github_info("https://github.com/junhoyeo/tokscale.git");
        assert_eq!(result, Some(("junhoyeo".to_string(), "tokscale".to_string())));
    }

    #[test]
    fn test_extract_github_info_non_github() {
        let result = extract_github_info("https://gitlab.com/owner/repo");
        assert_eq!(result, None);
    }
}
