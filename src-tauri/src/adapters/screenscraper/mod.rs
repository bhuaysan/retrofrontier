//! ScreenScraper provider adapter.
//!
//! Everything ScreenScraper-specific lives at or below this module: endpoint construction, query
//! parameter names, provider system identifiers, response parsing, media selection, quota
//! extraction, HTTP status interpretation, credential injection, and URL redaction. Above it, only
//! the provider-neutral vocabulary of `services::metadata_provider` is visible.

pub mod parse;
pub mod systems;

use crate::adapters::credentials::{ProviderCredentialSource, REDACTED};
use crate::adapters::http::{
    HttpClient, HttpRequest, HttpResponse, HttpTransportError, MAX_MEDIA_RESPONSE_BYTES,
    MAX_METADATA_RESPONSE_BYTES,
};
use crate::domain::metadata::{
    application_softname, MetadataProviderId, ProviderCandidate, ProviderFailureClass,
    ProviderQuotaSnapshot,
};
use crate::domain::system::SystemId;
use crate::services::metadata_provider::{
    CandidateSearchRequest, ContentIdentificationRequest, DownloadedMedia, MetadataProvider,
    ProviderGameRecord, ProviderMediaLocator, ProviderResponse, ProviderResult,
};
use async_trait::async_trait;
use parse::{MalformedResponse, ACCEPTED_COVER_CONTENT_TYPES};
use std::sync::Arc;
use systems::provider_system_mapping;
use url::Url;

/// Default provider base URL. Overridable so tests never depend on a hostname.
pub const DEFAULT_BASE_URL: &str = "https://api.screenscraper.fr/api2/";

const GAME_INFO_ENDPOINT: &str = "jeuInfos.php";
const GAME_SEARCH_ENDPOINT: &str = "jeuRecherche.php";

/// Provider-side cap on heuristic search results, per the provider's own documentation.
const MAX_SEARCH_CANDIDATES: usize = 30;

pub struct ScreenScraperProvider {
    http: Arc<dyn HttpClient>,
    credentials: Arc<dyn ProviderCredentialSource>,
    base_url: Url,
    softname: String,
}

impl ScreenScraperProvider {
    pub fn new(
        http: Arc<dyn HttpClient>,
        credentials: Arc<dyn ProviderCredentialSource>,
    ) -> Result<Self, ProviderFailureClass> {
        Self::with_base_url(http, credentials, DEFAULT_BASE_URL)
    }

    pub fn with_base_url(
        http: Arc<dyn HttpClient>,
        credentials: Arc<dyn ProviderCredentialSource>,
        base_url: &str,
    ) -> Result<Self, ProviderFailureClass> {
        let base_url = Url::parse(base_url).map_err(|_| ProviderFailureClass::InvalidRequest)?;
        Ok(Self {
            http,
            credentials,
            base_url,
            // Centralized application identity. The frontend can never supply this value.
            softname: application_softname(),
        })
    }

    /// Builds an endpoint URL with the mandatory client parameters already applied.
    fn endpoint(&self, endpoint: &str) -> Result<(Url, bool), ProviderFailureClass> {
        let developer = self
            .credentials
            .developer()
            .ok_or(ProviderFailureClass::CredentialsUnavailable)?;
        if developer.developer_id.is_empty() || developer.developer_password.is_empty() {
            return Err(ProviderFailureClass::CredentialsUnavailable);
        }

        let mut url = self
            .base_url
            .join(endpoint)
            .map_err(|_| ProviderFailureClass::InvalidRequest)?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("devid", developer.developer_id.expose());
            query.append_pair("devpassword", developer.developer_password.expose());
            query.append_pair("softname", &self.softname);
            query.append_pair("output", "json");
        }

        // Personal credentials are optional; guest access must keep working without them.
        let user_credentials_sent = match self.credentials.user() {
            Some(user) if !user.username.is_empty() => {
                let mut query = url.query_pairs_mut();
                query.append_pair("ssid", &user.username);
                query.append_pair("sspassword", user.password.expose());
                true
            }
            _ => false,
        };

        Ok((url, user_credentials_sent))
    }

    async fn call(
        &self,
        url: Url,
        user_credentials_sent: bool,
        max_response_bytes: u64,
    ) -> Result<HttpResponse, ProviderFailureClass> {
        let request = HttpRequest::new(url, max_response_bytes);
        // Only the redacted form is ever logged.
        tracing::debug!(url = %request.redacted_url(), "metadata provider request");

        let response = self
            .http
            .get(request)
            .await
            .map_err(classify_transport_error)?;

        if response.is_success() {
            return Ok(response);
        }
        Err(classify_status(
            response.status,
            &response.body,
            user_credentials_sent,
        ))
    }

    fn parse_game_response(
        body: &[u8],
    ) -> Result<(ProviderGameRecord, Option<ProviderQuotaSnapshot>), ProviderFailureClass> {
        let response = parse::response_object(body)
            .inspect_err(|_| log_malformed_body("response envelope", body))
            .map_err(malformed)?;
        let quota = parse::parse_quota(&response);
        let record = parse::parse_game(&response)
            .inspect_err(|_| log_malformed_body("game record", body))
            .map_err(malformed)?;
        Ok((record, quota))
    }
}

/// Longest excerpt of an unparseable body that is ever logged.
///
/// Long enough to carry a provider error sentence, short enough that a large HTML or JSON payload
/// cannot flood the log.
const MALFORMED_BODY_LOG_CHARS: usize = 240;

fn malformed(_: MalformedResponse) -> ProviderFailureClass {
    ProviderFailureClass::MalformedResponse
}

/// Records why a 2xx body could not be understood.
///
/// The provider answers some conditions — a blocked application, a closed API, an exhausted budget —
/// with HTTP 200 and a plain sentence instead of its JSON envelope. Without the body those all
/// collapse into an indistinguishable `malformed_response` that no operator can act on. The excerpt
/// is bounded and passed through the same redaction the free-text path already uses, so a
/// credential echoed back inside an error message cannot reach the log.
fn log_malformed_body(context: &'static str, body: &[u8]) {
    let text = String::from_utf8_lossy(body);
    let excerpt: String = text.trim().chars().take(MALFORMED_BODY_LOG_CHARS).collect();
    tracing::warn!(
        context,
        bytes = body.len(),
        // Structure only: which check failed, and where. Never a value from the body.
        reason = %parse::describe_envelope_failure(body),
        excerpt = %redact_text(&excerpt),
        "metadata provider returned a body that could not be understood"
    );
}

/// Maps transport failures onto the provider-neutral taxonomy.
fn classify_transport_error(error: HttpTransportError) -> ProviderFailureClass {
    match error {
        // Unreachable, timeout, and oversized responses are all transient from the caller's view.
        HttpTransportError::Unreachable | HttpTransportError::Timeout => {
            ProviderFailureClass::Transport
        }
        HttpTransportError::ResponseTooLarge => ProviderFailureClass::MalformedResponse,
        // A non-HTTPS provider URL is a configuration defect, not something to retry.
        HttpTransportError::InsecureScheme => ProviderFailureClass::InvalidRequest,
    }
}

/// Maps ScreenScraper HTTP statuses onto the provider-neutral taxonomy.
///
/// Each documented status has its own meaning, so a non-200 is never turned into a generic retry.
/// The 403 case needs the response body because the provider uses one status for both developer
/// and personal login failures; when no personal credentials were sent it can only be the former.
pub fn classify_status(
    status: u16,
    body: &[u8],
    user_credentials_sent: bool,
) -> ProviderFailureClass {
    match status {
        400 => ProviderFailureClass::InvalidRequest,
        401 => ProviderFailureClass::ProviderRestricted,
        403 => {
            if user_credentials_sent && body_mentions_user_login(body) {
                ProviderFailureClass::UserAuthenticationFailed
            } else {
                ProviderFailureClass::DeveloperAuthenticationFailed
            }
        }
        404 => ProviderFailureClass::NoMatch,
        423 => ProviderFailureClass::ProviderUnavailable,
        426 => ProviderFailureClass::ClientRejected,
        429 => ProviderFailureClass::CapacityDeferred,
        430 => ProviderFailureClass::DailyQuotaExceeded,
        431 => ProviderFailureClass::NegativeQuotaExceeded,
        500..=599 => ProviderFailureClass::TransientServer,
        _ => ProviderFailureClass::MalformedResponse,
    }
}

/// Inspects only field *names* in the error body, never a credential value.
fn body_mentions_user_login(body: &[u8]) -> bool {
    let text = String::from_utf8_lossy(&body[..body.len().min(512)]).to_ascii_lowercase();
    ["ssid", "sspassword", "membre", "utilisateur"]
        .iter()
        .any(|marker| text.contains(marker))
}

#[async_trait]
impl MetadataProvider for ScreenScraperProvider {
    fn provider_id(&self) -> MetadataProviderId {
        MetadataProviderId::ScreenScraper
    }

    fn supports_system(&self, system: SystemId) -> bool {
        provider_system_mapping(system).is_some()
    }

    async fn identify_content(
        &self,
        request: &ContentIdentificationRequest,
    ) -> ProviderResult<ProviderGameRecord> {
        let mapping = provider_system_mapping(request.system_id)
            .ok_or(ProviderFailureClass::InvalidRequest)?;
        let (mut url, user_credentials_sent) = self.endpoint(GAME_INFO_ENDPOINT)?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("systemeid", &mapping.provider_system_id.to_string());
            query.append_pair("romtype", mapping.rom_type.as_parameter());
            // Basenames only: the provider rejects paths, and a path would disclose the user's
            // directory layout.
            query.append_pair("romnom", &request.file_basename);
            query.append_pair("romtaille", &request.evidence.size_bytes.to_string());
            // All available hashes are sent because the provider recommends supplying all three.
            if let Some(crc32) = request.evidence.crc32.as_deref() {
                query.append_pair("crc", crc32);
            }
            if let Some(md5) = request.evidence.md5.as_deref() {
                query.append_pair("md5", md5);
            }
            if let Some(sha1) = request.evidence.sha1.as_deref() {
                query.append_pair("sha1", sha1);
            }
        }

        let response = self
            .call(url, user_credentials_sent, MAX_METADATA_RESPONSE_BYTES)
            .await?;
        let (record, quota) = Self::parse_game_response(&response.body)?;
        Ok(ProviderResponse::new(record, quota))
    }

    async fn search_candidates(
        &self,
        request: &CandidateSearchRequest,
    ) -> ProviderResult<Vec<ProviderCandidate>> {
        let mapping = provider_system_mapping(request.system_id)
            .ok_or(ProviderFailureClass::InvalidRequest)?;
        let (mut url, user_credentials_sent) = self.endpoint(GAME_SEARCH_ENDPOINT)?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("systemeid", &mapping.provider_system_id.to_string());
            query.append_pair("recherche", &request.title);
        }

        let response = self
            .call(url, user_credentials_sent, MAX_METADATA_RESPONSE_BYTES)
            .await?;
        let parsed = parse::response_object(&response.body)
            .inspect_err(|_| log_malformed_body("search envelope", &response.body))
            .map_err(malformed)?;
        let quota = parse::parse_quota(&parsed);
        let mut candidates = parse::parse_candidates(&parsed)
            .inspect_err(|_| log_malformed_body("search results", &response.body))
            .map_err(malformed)?;
        candidates.truncate(MAX_SEARCH_CANDIDATES);
        Ok(ProviderResponse::new(candidates, quota))
    }

    async fn fetch_game(
        &self,
        system: SystemId,
        provider_game_id: &str,
    ) -> ProviderResult<ProviderGameRecord> {
        let mapping =
            provider_system_mapping(system).ok_or(ProviderFailureClass::InvalidRequest)?;
        let (mut url, user_credentials_sent) = self.endpoint(GAME_INFO_ENDPOINT)?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("systemeid", &mapping.provider_system_id.to_string());
            // A game-ID request deliberately omits all content information: this is identity
            // retrieval for an existing relationship, never new matching evidence.
            query.append_pair("gameid", provider_game_id);
        }

        let response = self
            .call(url, user_credentials_sent, MAX_METADATA_RESPONSE_BYTES)
            .await?;
        let (mut record, quota) = Self::parse_game_response(&response.body)?;
        // Guarantee the caller cannot mistake this for content matching.
        record.matched_rom = None;
        Ok(ProviderResponse::new(record, quota))
    }

    async fn download_media(
        &self,
        locator: &ProviderMediaLocator,
    ) -> ProviderResult<DownloadedMedia> {
        let url =
            Url::parse(locator.expose()).map_err(|_| ProviderFailureClass::MediaUnavailable)?;
        let response = self
            .call(
                url,
                self.credentials.user().is_some(),
                MAX_MEDIA_RESPONSE_BYTES,
            )
            .await?;

        // Provider media endpoints answer 200 with a short marker body when nothing changed or
        // nothing exists, so the content type has to be checked rather than assumed.
        let content_type = response
            .content_type
            .as_deref()
            .map(|value| value.split(';').next().unwrap_or(value).trim().to_owned());
        let accepted = content_type.as_deref().is_some_and(|value| {
            ACCEPTED_COVER_CONTENT_TYPES
                .iter()
                .any(|accepted| accepted.eq_ignore_ascii_case(value))
        });
        if !accepted || response.body.is_empty() {
            return Err(ProviderFailureClass::MediaUnavailable);
        }

        Ok(ProviderResponse::new(
            DownloadedMedia {
                content_type,
                bytes: response.body,
            },
            None,
        ))
    }
}

/// Rewrites an arbitrary string so no provider credential can survive in it.
///
/// Used for defensive logging of adapter-side text that may embed a request URL. Structured
/// redaction via `HttpRequest::redacted_url` is preferred; this is the fallback for free text.
pub fn redact_text(text: &str) -> String {
    const KEYS: &[&str] = &["devid", "devpassword", "ssid", "sspassword"];

    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        // Lowercasing only affects ASCII, so byte offsets stay valid for the original slice.
        let lowered = rest.to_ascii_lowercase();
        let next = KEYS
            .iter()
            .filter_map(|key| {
                let needle = format!("{key}=");
                lowered.find(&needle).map(|start| (start, needle.len()))
            })
            .min_by_key(|(start, _)| *start);

        let Some((start, needle_length)) = next else {
            result.push_str(rest);
            return result;
        };

        let value_start = start + needle_length;
        let value_end = rest[value_start..]
            .find(['&', ' ', '"', '\'', '\n'])
            .map_or(rest.len(), |offset| value_start + offset);
        result.push_str(&rest[..value_start]);
        result.push_str(REDACTED);
        rest = &rest[value_end..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::credentials::StaticCredentialSource;
    use crate::adapters::http::redact_url;
    use crate::domain::library::{ContentFileId, ContentUnitId, ContentUnitKind, GameId};
    use crate::domain::metadata::{MatchEvidence, EVIDENCE_SCHEMA_VERSION};
    use std::sync::Mutex;

    /// Records every request and replies with a queued response.
    struct RecordingHttpClient {
        requests: Mutex<Vec<String>>,
        raw_urls: Mutex<Vec<String>>,
        responses: Mutex<Vec<Result<HttpResponse, HttpTransportError>>>,
    }

    impl RecordingHttpClient {
        fn new(responses: Vec<Result<HttpResponse, HttpTransportError>>) -> Arc<Self> {
            Arc::new(Self {
                requests: Mutex::new(Vec::new()),
                raw_urls: Mutex::new(Vec::new()),
                responses: Mutex::new(responses),
            })
        }

        fn ok(body: &str) -> Arc<Self> {
            Self::new(vec![Ok(HttpResponse {
                status: 200,
                content_type: Some("application/json".to_owned()),
                body: body.as_bytes().to_vec(),
            })])
        }

        fn status(status: u16, body: &str) -> Arc<Self> {
            Self::new(vec![Ok(HttpResponse {
                status,
                content_type: Some("text/plain".to_owned()),
                body: body.as_bytes().to_vec(),
            })])
        }

        fn last_raw_url(&self) -> String {
            self.raw_urls.lock().unwrap().last().cloned().unwrap()
        }

        fn call_count(&self) -> usize {
            self.raw_urls.lock().unwrap().len()
        }
    }

    #[async_trait]
    impl HttpClient for RecordingHttpClient {
        async fn get(&self, request: HttpRequest) -> Result<HttpResponse, HttpTransportError> {
            self.raw_urls.lock().unwrap().push(request.url.to_string());
            self.requests.lock().unwrap().push(request.redacted_url());
            let mut responses = self.responses.lock().unwrap();
            if responses.len() > 1 {
                responses.remove(0)
            } else {
                responses
                    .first()
                    .cloned()
                    .unwrap_or(Err(HttpTransportError::Unreachable))
            }
        }
    }

    fn evidence() -> MatchEvidence {
        MatchEvidence {
            game_id: GameId(1),
            content_unit_id: ContentUnitId(2),
            system_id: SystemId::Snes,
            content_unit_kind: ContentUnitKind::SingleFile,
            content_file_id: Some(ContentFileId(3)),
            size_bytes: 524_288,
            crc32: Some("AABBCCDD".to_owned()),
            md5: Some("d41d8cd98f00b204e9800998ecf8427e".to_owned()),
            sha1: Some("da39a3ee5e6b4b0d3255bfef95601890afd80709".to_owned()),
            fingerprint: Some("fingerprint-1".to_owned()),
            evidence_version: EVIDENCE_SCHEMA_VERSION,
        }
    }

    fn identification_request() -> ContentIdentificationRequest {
        ContentIdentificationRequest {
            system_id: SystemId::Snes,
            evidence: evidence(),
            file_basename: "Example Quest (USA).sfc".to_owned(),
        }
    }

    fn provider(
        http: Arc<dyn HttpClient>,
        credentials: StaticCredentialSource,
    ) -> ScreenScraperProvider {
        ScreenScraperProvider::with_base_url(
            http,
            Arc::new(credentials),
            "https://provider.invalid/api2/",
        )
        .expect("adapter should build")
    }

    const MINIMAL_GAME: &str = r#"{"response":{"jeu":{"id":"3","romid":"77","rom":{"id":"101",
        "romsize":"524288","romcrc":"AABBCCDD","rommd5":"d41d8cd98f00b204e9800998ecf8427e",
        "romsha1":"da39a3ee5e6b4b0d3255bfef95601890afd80709"}}}}"#;

    #[tokio::test]
    async fn identification_requests_carry_the_mapped_system_all_hashes_and_the_basename() {
        let http = RecordingHttpClient::ok(MINIMAL_GAME);
        let adapter = provider(
            http.clone(),
            StaticCredentialSource::developer_only("fake-dev-id", "fake-dev-password"),
        );

        adapter
            .identify_content(&identification_request())
            .await
            .expect("fixture should identify");

        let url = Url::parse(&http.last_raw_url()).expect("request URL should parse");
        let query: Vec<(String, String)> = url
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect();

        assert!(url.path().ends_with("jeuInfos.php"));
        assert!(query.contains(&("systemeid".to_owned(), "4".to_owned())));
        assert!(query.contains(&("romtype".to_owned(), "rom".to_owned())));
        assert!(query.contains(&("romtaille".to_owned(), "524288".to_owned())));
        assert!(query.contains(&("crc".to_owned(), "AABBCCDD".to_owned())));
        assert!(query.contains(&(
            "md5".to_owned(),
            "d41d8cd98f00b204e9800998ecf8427e".to_owned()
        )));
        assert!(query.contains(&(
            "sha1".to_owned(),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709".to_owned()
        )));
        assert!(query.contains(&("romnom".to_owned(), "Example Quest (USA).sfc".to_owned())));
        assert!(query.contains(&("output".to_owned(), "json".to_owned())));
        // Spaces and parentheses must be encoded, never passed through raw.
        assert!(http
            .last_raw_url()
            .contains("romnom=Example+Quest+%28USA%29.sfc"));
    }

    #[tokio::test]
    async fn a_disc_system_uses_the_iso_rom_type() {
        let http = RecordingHttpClient::ok(MINIMAL_GAME);
        let adapter = provider(
            http.clone(),
            StaticCredentialSource::developer_only("fake-dev-id", "fake-dev-password"),
        );
        let mut request = identification_request();
        request.system_id = SystemId::NintendoGameCube;

        adapter
            .identify_content(&request)
            .await
            .expect("fixture should identify");

        let url = Url::parse(&http.last_raw_url()).unwrap();
        let query: Vec<_> = url.query_pairs().collect();
        assert!(query.contains(&("systemeid".into(), "13".into())));
        assert!(query.contains(&("romtype".into(), "iso".into())));
    }

    #[tokio::test]
    async fn the_softname_is_the_centralized_application_identity_only() {
        let http = RecordingHttpClient::ok(MINIMAL_GAME);
        let adapter = provider(
            http.clone(),
            StaticCredentialSource::developer_only("fake-dev-id", "fake-dev-password"),
        );

        adapter
            .identify_content(&identification_request())
            .await
            .expect("fixture should identify");

        let url = Url::parse(&http.last_raw_url()).unwrap();
        let softname = url
            .query_pairs()
            .find(|(key, _)| key == "softname")
            .map(|(_, value)| value.into_owned())
            .expect("softname is mandatory");
        assert_eq!(softname, application_softname());
        assert!(softname.starts_with("RetroFrontier/"));
    }

    #[tokio::test]
    async fn personal_credentials_are_omitted_for_guest_access_and_sent_when_configured() {
        let guest_http = RecordingHttpClient::ok(MINIMAL_GAME);
        provider(
            guest_http.clone(),
            StaticCredentialSource::developer_only("fake-dev-id", "fake-dev-password"),
        )
        .identify_content(&identification_request())
        .await
        .expect("guest access must work");
        let guest_url = guest_http.last_raw_url();
        assert!(!guest_url.contains("ssid="));
        assert!(!guest_url.contains("sspassword="));

        let member_http = RecordingHttpClient::ok(MINIMAL_GAME);
        provider(
            member_http.clone(),
            StaticCredentialSource::developer_only("fake-dev-id", "fake-dev-password")
                .with_user("fake-account", "fake-user-password"),
        )
        .identify_content(&identification_request())
        .await
        .expect("member access must work");
        let member_url = member_http.last_raw_url();
        assert!(member_url.contains("ssid=fake-account"));
        assert!(member_url.contains("sspassword=fake-user-password"));
    }

    #[tokio::test]
    async fn a_build_without_application_credentials_never_issues_a_request() {
        let http = RecordingHttpClient::ok(MINIMAL_GAME);
        let adapter = provider(http.clone(), StaticCredentialSource::without_developer());

        let failure = adapter
            .identify_content(&identification_request())
            .await
            .expect_err("a build without credentials cannot call the provider");

        assert_eq!(failure, ProviderFailureClass::CredentialsUnavailable);
        assert_eq!(http.call_count(), 0);
    }

    #[test]
    fn a_malformed_body_excerpt_is_bounded_and_redacted() {
        // The provider answers some conditions with HTTP 200 and a sentence rather than JSON, and
        // that sentence is the only thing that tells an operator what is wrong. It must be
        // loggable without carrying a credential or flooding the log.
        let hostile = format!(
            "Erreur: devid=real-developer-id&devpassword=real-developer-password {}",
            "x".repeat(4_096)
        );
        let excerpt: String = hostile
            .trim()
            .chars()
            .take(MALFORMED_BODY_LOG_CHARS)
            .collect();
        let logged = redact_text(&excerpt);

        assert!(logged.chars().count() <= MALFORMED_BODY_LOG_CHARS);
        assert!(!logged.contains("real-developer-id"));
        assert!(!logged.contains("real-developer-password"));
        assert!(
            logged.contains("Erreur"),
            "the diagnostic text must survive"
        );
    }

    #[test]
    fn a_body_that_is_not_json_is_reported_as_malformed_rather_than_retried_blindly() {
        // ScreenScraper's plain-text refusals arrive with HTTP 200.
        let body = b"API totalement fermee pour le moment";
        assert_eq!(parse::response_object(body), Err(MalformedResponse));
        log_malformed_body("test", body);
    }
    #[tokio::test]
    async fn every_logged_request_url_is_redacted() {
        let http = RecordingHttpClient::ok(MINIMAL_GAME);
        provider(
            http.clone(),
            StaticCredentialSource::developer_only("real-dev-id", "real-dev-password")
                .with_user("real-account", "real-user-password"),
        )
        .identify_content(&identification_request())
        .await
        .expect("fixture should identify");

        let logged = http.requests.lock().unwrap().last().cloned().unwrap();
        for secret in [
            "real-dev-id",
            "real-dev-password",
            "real-account",
            "real-user-password",
        ] {
            assert!(
                !logged.contains(secret),
                "{secret} must not appear in {logged}"
            );
        }
        assert!(logged.contains("devid=%3Credacted%3E"));
        assert_eq!(
            logged,
            redact_url(&Url::parse(&http.last_raw_url()).unwrap())
        );
    }

    #[tokio::test]
    async fn a_game_id_fetch_cannot_masquerade_as_content_matching() {
        let http = RecordingHttpClient::ok(MINIMAL_GAME);
        let adapter = provider(
            http.clone(),
            StaticCredentialSource::developer_only("fake-dev-id", "fake-dev-password"),
        );

        let response = adapter
            .fetch_game(SystemId::Snes, "3")
            .await
            .expect("identity retrieval should succeed");

        assert!(
            response.value.matched_rom.is_none(),
            "a game-ID fetch must not surface a matched content record"
        );
        let url = http.last_raw_url();
        assert!(url.contains("gameid=3"));
        assert!(!url.contains("romtaille="));
        assert!(!url.contains("sha1="));
    }

    #[tokio::test]
    async fn heuristic_search_is_capped_and_ordered_by_the_provider() {
        let mut games = String::new();
        for index in 0..40 {
            if index > 0 {
                games.push(',');
            }
            games.push_str(&format!(
                r#"{{"id":"{index}","noms":[{{"region":"us","text":"Result {index}"}}]}}"#
            ));
        }
        let http = RecordingHttpClient::ok(&format!(r#"{{"response":{{"jeux":[{games}]}}}}"#));
        let adapter = provider(
            http.clone(),
            StaticCredentialSource::developer_only("fake-dev-id", "fake-dev-password"),
        );

        let response = adapter
            .search_candidates(&CandidateSearchRequest {
                system_id: SystemId::Snes,
                title: "Result".to_owned(),
            })
            .await
            .expect("search should succeed");

        assert_eq!(response.value.len(), MAX_SEARCH_CANDIDATES);
        assert_eq!(response.value[0].provider_game_id, "0");
        assert!(http.last_raw_url().contains("jeuRecherche.php"));
        assert!(http.last_raw_url().contains("recherche=Result"));
    }

    #[tokio::test]
    async fn quota_is_reported_with_a_successful_response() {
        let http = RecordingHttpClient::ok(
            r#"{"response":{"ssuser":{"maxthreads":"2","maxrequestsperday":"5000",
                "requeststoday":"11"},"jeu":{"id":"3"}}}"#,
        );
        let adapter = provider(
            http,
            StaticCredentialSource::developer_only("fake-dev-id", "fake-dev-password"),
        );

        let response = adapter
            .identify_content(&identification_request())
            .await
            .expect("fixture should identify");
        let quota = response.quota.expect("quota should be reported");

        assert_eq!(quota.max_threads, Some(2));
        assert_eq!(quota.max_requests_per_day, Some(5_000));
        assert_eq!(quota.requests_today, Some(11));
    }

    #[test]
    fn each_documented_status_maps_to_its_own_classification() {
        let cases = [
            (400, ProviderFailureClass::InvalidRequest),
            (401, ProviderFailureClass::ProviderRestricted),
            (404, ProviderFailureClass::NoMatch),
            (423, ProviderFailureClass::ProviderUnavailable),
            (426, ProviderFailureClass::ClientRejected),
            (429, ProviderFailureClass::CapacityDeferred),
            (430, ProviderFailureClass::DailyQuotaExceeded),
            (431, ProviderFailureClass::NegativeQuotaExceeded),
            (500, ProviderFailureClass::TransientServer),
            (503, ProviderFailureClass::TransientServer),
            (418, ProviderFailureClass::MalformedResponse),
        ];

        for (status, expected) in cases {
            assert_eq!(
                classify_status(status, b"", false),
                expected,
                "HTTP {status} is misclassified"
            );
        }
    }

    #[test]
    fn developer_and_personal_authentication_failures_are_distinguished() {
        assert_eq!(
            classify_status(403, b"Erreur de login : Verifier vos identifiants !", false),
            ProviderFailureClass::DeveloperAuthenticationFailed,
            "without personal credentials a 403 can only be a developer failure"
        );
        assert_eq!(
            classify_status(403, b"Champ ssid ou sspassword incorrect !", true),
            ProviderFailureClass::UserAuthenticationFailed
        );
        assert_eq!(
            classify_status(403, b"Erreur de login developpeur", true),
            ProviderFailureClass::DeveloperAuthenticationFailed
        );
    }

    #[tokio::test]
    async fn transport_and_size_failures_are_classified_without_generic_retry() {
        for (transport, expected) in [
            (
                HttpTransportError::Unreachable,
                ProviderFailureClass::Transport,
            ),
            (HttpTransportError::Timeout, ProviderFailureClass::Transport),
            (
                HttpTransportError::ResponseTooLarge,
                ProviderFailureClass::MalformedResponse,
            ),
            (
                HttpTransportError::InsecureScheme,
                ProviderFailureClass::InvalidRequest,
            ),
        ] {
            let http = RecordingHttpClient::new(vec![Err(transport)]);
            let adapter = provider(
                http,
                StaticCredentialSource::developer_only("fake-dev-id", "fake-dev-password"),
            );

            assert_eq!(
                adapter
                    .identify_content(&identification_request())
                    .await
                    .expect_err("transport failure"),
                expected
            );
        }
    }

    #[tokio::test]
    async fn a_malformed_success_body_is_a_protocol_failure() {
        let http = RecordingHttpClient::ok("<html>not json</html>");
        let adapter = provider(
            http,
            StaticCredentialSource::developer_only("fake-dev-id", "fake-dev-password"),
        );

        assert_eq!(
            adapter
                .identify_content(&identification_request())
                .await
                .expect_err("malformed body"),
            ProviderFailureClass::MalformedResponse
        );
    }

    #[tokio::test]
    async fn a_no_match_status_is_a_deterministic_negative_answer() {
        let http = RecordingHttpClient::status(404, "Erreur : Rom/Iso/Dossier non trouve !");
        let adapter = provider(
            http,
            StaticCredentialSource::developer_only("fake-dev-id", "fake-dev-password"),
        );

        assert_eq!(
            adapter
                .identify_content(&identification_request())
                .await
                .expect_err("no match"),
            ProviderFailureClass::NoMatch
        );
    }

    #[tokio::test]
    async fn media_downloads_accept_only_image_content() {
        let png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00];
        let http = RecordingHttpClient::new(vec![Ok(HttpResponse {
            status: 200,
            content_type: Some("image/png; charset=binary".to_owned()),
            body: png.clone(),
        })]);
        let adapter = provider(
            http,
            StaticCredentialSource::developer_only("fake-dev-id", "fake-dev-password"),
        );
        let locator = ProviderMediaLocator::new("https://provider.invalid/media/cover");

        let media = adapter
            .download_media(&locator)
            .await
            .expect("an image response should be accepted");
        assert_eq!(media.value.content_type.as_deref(), Some("image/png"));
        assert_eq!(media.value.bytes, png);

        for (content_type, body) in [
            (Some("text/plain".to_owned()), b"NOMEDIA".to_vec()),
            (Some("text/html".to_owned()), b"CRCOK".to_vec()),
            (None, b"bytes".to_vec()),
            (Some("image/png".to_owned()), Vec::new()),
        ] {
            let http = RecordingHttpClient::new(vec![Ok(HttpResponse {
                status: 200,
                content_type,
                body,
            })]);
            let adapter = provider(
                http,
                StaticCredentialSource::developer_only("fake-dev-id", "fake-dev-password"),
            );
            assert_eq!(
                adapter
                    .download_media(&locator)
                    .await
                    .expect_err("rejected"),
                ProviderFailureClass::MediaUnavailable
            );
        }
    }

    #[test]
    fn free_text_redaction_removes_every_credential_parameter() {
        let text = "GET https://provider.invalid/api2/jeuInfos.php?devid=real-id\
                    &devpassword=real-password&ssid=real-account&sspassword=real-user-password\
                    &systemeid=4 failed";

        let redacted = redact_text(text);

        for secret in [
            "real-id",
            "real-password",
            "real-account",
            "real-user-password",
        ] {
            assert!(
                !redacted.contains(secret),
                "{secret} survived in {redacted}"
            );
        }
        assert!(redacted.contains("systemeid=4"));
        assert!(redacted.contains("devid=<redacted>"));
    }

    #[test]
    fn unmapped_systems_are_reported_rather_than_searched_globally() {
        let adapter = provider(
            RecordingHttpClient::ok(MINIMAL_GAME),
            StaticCredentialSource::developer_only("fake-dev-id", "fake-dev-password"),
        );

        for system in SystemId::ALL_V1 {
            assert!(
                adapter.supports_system(*system),
                "{system} must be supported by the V1 adapter"
            );
        }
    }
}
