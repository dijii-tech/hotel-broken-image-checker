use anyhow::Result;
use reqwest::Client;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

/// Status codes that should NOT trigger a retry (permanent failures)
const NON_RETRYABLE_STATUS_CODES: [u16; 6] = [
    400, // Bad Request
    401, // Unauthorized
    403, // Forbidden
    404, // Not Found
    410, // Gone
    451, // Unavailable For Legal Reasons
];

/// URL checker with concurrent request handling
pub struct UrlChecker {
    client: Client,
    semaphore: Arc<Semaphore>,
    retry_attempts: u32,
    retry_delay: Duration,
}

/// Result of checking a single URL
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub id: i64,
    pub url: String,
    pub is_valid: bool,
    pub status_code: Option<u16>,
    pub error: Option<String>,
    pub is_retryable: bool,
}

impl UrlChecker {
    /// Create a new URL checker with specified concurrency, timeout, and retry settings
    pub fn new(
        concurrency: usize,
        timeout_secs: u64,
        retry_attempts: u32,
        retry_delay_secs: u64,
    ) -> Result<Self> {
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
            retry_attempts,
            retry_delay: Duration::from_secs(retry_delay_secs),
        })
    }

    /// Check a batch of URLs concurrently with 3-phase retry mechanism
    ///
    /// Phase 1: Normal check - all URLs checked concurrently
    /// Phase 2: First retry - retryable failures checked again
    /// Phase 3: Final retry - remaining retryable failures checked after delay
    pub async fn check_batch(&self, urls: Vec<(i64, String)>) -> Vec<CheckResult> {
        // Phase 1: Initial check
        let initial_results = self.check_urls_internal(urls).await;

        // Separate successful and retryable results
        let (mut final_results, retryable): (Vec<_>, Vec<_>) = initial_results
            .into_iter()
            .partition(|r| r.is_valid || !r.is_retryable);

        if retryable.is_empty() {
            return final_results;
        }

        // Phase 2: First retry (immediate)
        if self.retry_attempts >= 1 {
            let retry_urls: Vec<(i64, String)> = retryable
                .iter()
                .map(|r| (r.id, r.url.clone()))
                .collect();

            info!(
                "Phase 2: Retrying {} URLs with retryable errors",
                retry_urls.len()
            );

            let retry_results = self.check_urls_internal(retry_urls).await;

            let (succeeded, still_retryable): (Vec<_>, Vec<_>) = retry_results
                .into_iter()
                .partition(|r| r.is_valid || !r.is_retryable);

            final_results.extend(succeeded);

            if still_retryable.is_empty() {
                return final_results;
            }

            // Phase 3: Final retry with delay
            if self.retry_attempts >= 2 {
                let retry_urls: Vec<(i64, String)> = still_retryable
                    .iter()
                    .map(|r| (r.id, r.url.clone()))
                    .collect();

                info!(
                    "Phase 3: Waiting {:?} before final retry of {} URLs",
                    self.retry_delay,
                    retry_urls.len()
                );

                tokio::time::sleep(self.retry_delay).await;

                let final_retry_results = self.check_urls_internal(retry_urls).await;
                final_results.extend(final_retry_results);
            } else {
                final_results.extend(still_retryable);
            }
        } else {
            final_results.extend(retryable);
        }

        final_results
    }

    /// Internal method to check URLs without retry logic
    async fn check_urls_internal(&self, urls: Vec<(i64, String)>) -> Vec<CheckResult> {
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
                            "Broken URL [ID: {}]: {} - {:?} (retryable: {})",
                            result.id, result.url, result.error, result.is_retryable
                        );
                    }

                    result
                }
            })
            .collect();

        futures::future::join_all(futures).await
    }
}

/// Check if a status code is retryable
/// All errors are retryable EXCEPT permanent failures like 404
fn is_retryable_status(status_code: u16) -> bool {
    !NON_RETRYABLE_STATUS_CODES.contains(&status_code)
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
            is_retryable: false,
        };
    }

    // Try HEAD request first
    match client.head(url).send().await {
        Ok(response) => {
            let status = response.status();
            let status_code = status.as_u16();

            // 405 Method Not Allowed - server doesn't support HEAD, try GET
            if status_code == 405 {
                match client.get(url).send().await {
                    Ok(response) => {
                        let status = response.status();
                        let status_code = status.as_u16();
                        let is_valid = status.is_success() || status.is_redirection();
                        CheckResult {
                            id,
                            url: url.to_string(),
                            is_valid,
                            status_code: Some(status_code),
                            error: if is_valid {
                                None
                            } else {
                                Some(format!("HTTP {}", status_code))
                            },
                            is_retryable: !is_valid && is_retryable_status(status_code),
                        }
                    }
                    Err(e) => {
                        let is_timeout = e.is_timeout();
                        CheckResult {
                            id,
                            url: url.to_string(),
                            is_valid: false,
                            status_code: None,
                            error: Some(format!("GET request failed: {}", e)),
                            is_retryable: is_timeout, // Timeouts are retryable
                        }
                    }
                }
            } else {
                let is_valid = status.is_success() || status.is_redirection();
                CheckResult {
                    id,
                    url: url.to_string(),
                    is_valid,
                    status_code: Some(status_code),
                    error: if is_valid {
                        None
                    } else {
                        Some(format!("HTTP {}", status_code))
                    },
                    is_retryable: !is_valid && is_retryable_status(status_code),
                }
            }
        }
        Err(e) => {
            // Connection errors, timeouts, etc.
            let is_timeout = e.is_timeout();
            let error_msg = if is_timeout {
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
                is_retryable: is_timeout, // Timeouts are retryable
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_check_valid_url() {
        let checker = UrlChecker::new(10, 10, 2, 10).unwrap();
        let results = checker
            .check_batch(vec![(1, "https://www.google.com".to_string())])
            .await;
        assert!(results[0].is_valid);
    }

    #[tokio::test]
    async fn test_check_invalid_url() {
        let checker = UrlChecker::new(10, 5, 0, 10).unwrap();
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
        let checker = UrlChecker::new(10, 5, 0, 10).unwrap();
        let results = checker
            .check_batch(vec![(1, "ftp://example.com".to_string())])
            .await;
        assert!(!results[0].is_valid);
        assert!(!results[0].is_retryable); // Invalid scheme is not retryable
    }

    #[test]
    fn test_retryable_status_codes() {
        // 404 should NOT be retryable
        assert!(!is_retryable_status(404));
        assert!(!is_retryable_status(400));
        assert!(!is_retryable_status(403));

        // 503, 502, 500 etc should be retryable
        assert!(is_retryable_status(503));
        assert!(is_retryable_status(502));
        assert!(is_retryable_status(500));
        assert!(is_retryable_status(429));
    }
}
