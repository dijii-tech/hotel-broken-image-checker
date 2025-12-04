use anyhow::Result;
use reqwest::Client;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing::{debug, warn};

/// URL checker with concurrent request handling
pub struct UrlChecker {
    client: Client,
    semaphore: Arc<Semaphore>,
}

/// Result of checking a single URL
#[derive(Debug)]
pub struct CheckResult {
    pub id: i64,
    pub url: String,
    pub is_valid: bool,
    pub status_code: Option<u16>,
    pub error: Option<String>,
}

impl UrlChecker {
    /// Create a new URL checker with specified concurrency and timeout
    pub fn new(concurrency: usize, timeout_secs: u64) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(concurrency)
            .user_agent("Mozilla/5.0 (compatible; BrokenImageChecker/1.0)")
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()?;

        let semaphore = Arc::new(Semaphore::new(concurrency));

        Ok(Self {
            client,
            semaphore,
        })
    }

    /// Check a batch of URLs concurrently
    pub async fn check_batch(&self, urls: Vec<(i64, String)>) -> Vec<CheckResult> {
        let semaphore = self.semaphore.clone();
        let client = self.client.clone();

        let futures: Vec<_> = urls
            .into_iter()
            .map(|(id, url)| {
                let client = client.clone();
                let semaphore = semaphore.clone();

                async move {
                    // Acquire semaphore permit to limit concurrency
                    let _permit = semaphore.acquire().await.unwrap();

                    // Check the URL
                    let result = check_single_url(&client, id, &url).await;

                    if !result.is_valid {
                        debug!(
                            "Broken URL [ID: {}]: {} - {:?}",
                            result.id, result.url, result.error
                        );
                    }

                    result
                }
            })
            .collect();

        futures::future::join_all(futures).await
    }
}

/// Check a single URL with timeout
async fn check_single_url(client: &Client, id: i64, url: &str) -> CheckResult {
    // Validate URL format first
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return CheckResult {
            id,
            url: url.to_string(),
            is_valid: false,
            status_code: None,
            error: Some("Invalid URL scheme".to_string()),
        };
    }

    // Try HEAD request first
    match client.head(url).send().await {
        Ok(response) => {
            let status = response.status();

            // 405 Method Not Allowed - server doesn't support HEAD, try GET
            if status.as_u16() == 405 {
                match client.get(url).send().await {
                    Ok(response) => {
                        let status = response.status();
                        CheckResult {
                            id,
                            url: url.to_string(),
                            is_valid: status.is_success() || status.is_redirection(),
                            status_code: Some(status.as_u16()),
                            error: None,
                        }
                    }
                    Err(e) => CheckResult {
                        id,
                        url: url.to_string(),
                        is_valid: false,
                        status_code: None,
                        error: Some(format!("GET request failed: {}", e)),
                    },
                }
            } else {
                CheckResult {
                    id,
                    url: url.to_string(),
                    is_valid: status.is_success() || status.is_redirection(),
                    status_code: Some(status.as_u16()),
                    error: None,
                }
            }
        }
        Err(e) => {
            // Connection errors, timeouts, etc.
            let error_msg = if e.is_timeout() {
                "Request timed out".to_string()
            } else if e.is_connect() {
                "Connection failed".to_string()
            } else {
                format!("Request failed: {}", e)
            };

            warn!("Failed to check URL {}: {}", url, error_msg);

            CheckResult {
                id,
                url: url.to_string(),
                is_valid: false,
                status_code: None,
                error: Some(error_msg),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_check_valid_url() {
        let checker = UrlChecker::new(10, 10).unwrap();
        let results = checker
            .check_batch(vec![(1, "https://www.google.com".to_string())])
            .await;
        assert!(results[0].is_valid);
    }

    #[tokio::test]
    async fn test_check_invalid_url() {
        let checker = UrlChecker::new(10, 5).unwrap();
        let results = checker
            .check_batch(vec![(
                1,
                "https://this-domain-does-not-exist-12345.com".to_string(),
            )])
            .await;
        assert!(!results[0].is_valid);
    }

    #[tokio::test]
    async fn test_check_invalid_scheme() {
        let checker = UrlChecker::new(10, 5).unwrap();
        let results = checker
            .check_batch(vec![(1, "ftp://example.com".to_string())])
            .await;
        assert!(!results[0].is_valid);
    }
}
