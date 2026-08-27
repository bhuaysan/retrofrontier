//! Minimal, mockable HTTP boundary for metadata providers.
//!
//! This is deliberately not a general networking framework. It offers exactly what M5 needs: one
//! bounded HTTPS GET that returns status, content type, and a size-capped body, plus URL redaction
//! so a credential-bearing query string can never reach a log, an error, or a panic message.

use crate::domain::metadata::application_user_agent;
use async_trait::async_trait;
use std::fmt;
use std::time::Duration;
use thiserror::Error;
use url::Url;

/// Response cap for metadata calls. Provider bodies are small; anything larger is a defect.
pub const MAX_METADATA_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;

/// Response cap for one cached cover asset.
pub const MAX_MEDIA_RESPONSE_BYTES: u64 = 8 * 1024 * 1024;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Query parameters whose values are secret or personally identifying.
///
/// Kept here rather than in the provider adapter so every transport diagnostic is redacted even if
/// a future adapter forgets to redact its own errors.
const SENSITIVE_QUERY_KEYS: &[&str] = &["devid", "devpassword", "ssid", "sspassword"];

/// Rewrites a URL for human consumption, replacing sensitive query values.
///
/// The parameter *names* are preserved because they are useful in diagnostics and are not secret;
/// only the values are removed.
pub fn redact_url(url: &Url) -> String {
    let mut redacted = url.clone();
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| {
            let key = key.into_owned();
            let value = if SENSITIVE_QUERY_KEYS
                .iter()
                .any(|sensitive| key.eq_ignore_ascii_case(sensitive))
            {
                crate::adapters::credentials::REDACTED.to_owned()
            } else {
                value.into_owned()
            };
            (key, value)
        })
        .collect();

    if pairs.is_empty() {
        redacted.set_query(None);
    } else {
        redacted.query_pairs_mut().clear().extend_pairs(pairs);
    }
    redacted.set_fragment(None);
    redacted.to_string()
}

/// A bounded provider GET request.
///
/// `Debug` renders the redacted URL, so tracing a request cannot leak a credential.
#[derive(Clone)]
pub struct HttpRequest {
    pub url: Url,
    pub max_response_bytes: u64,
}

impl HttpRequest {
    pub fn new(url: Url, max_response_bytes: u64) -> Self {
        Self {
            url,
            max_response_bytes,
        }
    }

    /// The URL with sensitive query values removed. The only form safe to log.
    pub fn redacted_url(&self) -> String {
        redact_url(&self.url)
    }
}

impl fmt::Debug for HttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpRequest")
            .field("url", &self.redacted_url())
            .field("max_response_bytes", &self.max_response_bytes)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Transport-level failure.
///
/// Variants carry no URL, host, header, or body so that the error itself can never republish a
/// credential that appeared in the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum HttpTransportError {
    #[error("the provider could not be reached")]
    Unreachable,
    #[error("the provider request timed out")]
    Timeout,
    #[error("provider requests must use HTTPS")]
    InsecureScheme,
    #[error("the provider response exceeded the permitted size")]
    ResponseTooLarge,
}

#[async_trait]
pub trait HttpClient: Send + Sync {
    async fn get(&self, request: HttpRequest) -> Result<HttpResponse, HttpTransportError>;
}

/// Production HTTPS client with bounded timeouts and an explicit user agent.
pub struct ReqwestHttpClient {
    client: reqwest::Client,
}

impl ReqwestHttpClient {
    pub fn new() -> Result<Self, HttpTransportError> {
        let client = reqwest::Client::builder()
            .user_agent(application_user_agent())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            // Provider media links may redirect, but never off HTTPS and never many times.
            .redirect(reqwest::redirect::Policy::limited(3))
            .https_only(true)
            .build()
            .map_err(|_| HttpTransportError::Unreachable)?;
        Ok(Self { client })
    }
}

#[async_trait]
impl HttpClient for ReqwestHttpClient {
    async fn get(&self, request: HttpRequest) -> Result<HttpResponse, HttpTransportError> {
        if request.url.scheme() != "https" {
            return Err(HttpTransportError::InsecureScheme);
        }

        let mut response = self
            .client
            .get(request.url.clone())
            .send()
            .await
            .map_err(classify_reqwest_error)?;
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);

        // Refuse an oversized body before allocating it, then enforce the cap again while reading
        // so a missing or lying Content-Length cannot bypass the limit.
        if response
            .content_length()
            .is_some_and(|length| length > request.max_response_bytes)
        {
            return Err(HttpTransportError::ResponseTooLarge);
        }

        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(classify_reqwest_error)? {
            if body.len() as u64 + chunk.len() as u64 > request.max_response_bytes {
                return Err(HttpTransportError::ResponseTooLarge);
            }
            body.extend_from_slice(&chunk);
        }

        Ok(HttpResponse {
            status,
            content_type,
            body,
        })
    }
}

fn classify_reqwest_error(error: reqwest::Error) -> HttpTransportError {
    if error.is_timeout() {
        HttpTransportError::Timeout
    } else {
        HttpTransportError::Unreachable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_query_values_are_redacted_and_parameter_names_are_kept() {
        let url = Url::parse(
            "https://provider.invalid/api2/jeuInfos.php?devid=real-id&devpassword=real-password\
             &softname=RetroFrontier%2F0.1.0&ssid=account&sspassword=real-user-password\
             &systemeid=4&sha1=DA39",
        )
        .expect("fixture URL should parse");

        let redacted = redact_url(&url);

        assert!(!redacted.contains("real-id"));
        assert!(!redacted.contains("real-password"));
        assert!(!redacted.contains("account"));
        assert!(!redacted.contains("real-user-password"));
        assert!(redacted.contains("devid=%3Credacted%3E"));
        assert!(redacted.contains("devpassword=%3Credacted%3E"));
        assert!(redacted.contains("ssid=%3Credacted%3E"));
        assert!(redacted.contains("sspassword=%3Credacted%3E"));
        assert!(redacted.contains("systemeid=4"));
        assert!(redacted.contains("sha1=DA39"));
        assert!(redacted.contains("softname=RetroFrontier"));
    }

    #[test]
    fn redaction_is_case_insensitive_and_handles_empty_queries() {
        let url = Url::parse("https://provider.invalid/api?DevPassword=real&Other=1")
            .expect("fixture URL should parse");
        assert!(!redact_url(&url).contains("real"));

        let plain = Url::parse("https://provider.invalid/media/cover.png#fragment")
            .expect("fixture URL should parse");
        assert_eq!(
            redact_url(&plain),
            "https://provider.invalid/media/cover.png"
        );
    }

    #[test]
    fn request_debug_output_never_contains_a_credential() {
        let request = HttpRequest::new(
            Url::parse("https://provider.invalid/api?devpassword=real-password")
                .expect("fixture URL should parse"),
            MAX_METADATA_RESPONSE_BYTES,
        );

        let rendered = format!("{request:?}");
        assert!(!rendered.contains("real-password"));
        assert!(rendered.contains("%3Credacted%3E"));
    }

    #[test]
    fn transport_errors_carry_no_request_detail() {
        for error in [
            HttpTransportError::Unreachable,
            HttpTransportError::Timeout,
            HttpTransportError::InsecureScheme,
            HttpTransportError::ResponseTooLarge,
        ] {
            let rendered = format!("{error} {error:?}");
            assert!(!rendered.contains("devpassword"));
            assert!(!rendered.contains("http"));
        }
    }
}
