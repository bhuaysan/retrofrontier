//! ScreenScraper response parsing and normalization.
//!
//! The provider's Web API v2 is documented as beta and may change without notice, and the observed
//! schema mixes strings and numbers and has at least one inconsistent field spelling. Parsing is
//! therefore deliberately tolerant: unknown fields are ignored, missing optional fields are
//! accepted, numbers are read from either representation, and a structurally unusable body is
//! rejected as a protocol failure rather than silently producing empty metadata.
//!
//! Nothing here is persisted verbatim. The output is the provider-neutral record defined by
//! `services::metadata_provider`.

use crate::domain::metadata::{NormalizedMetadata, ProviderCandidate, ProviderQuotaSnapshot};
use crate::services::metadata_provider::{
    ProviderCoverDescriptor, ProviderGameRecord, ProviderMediaLocator, ProviderRomRecord,
};
use serde_json::Value;

/// Provider media types accepted as the single V1 primary front cover, most preferred first.
///
/// Only front box art is eligible. Screenshots, wheels/logos, fan art, back covers, physical media,
/// videos, and manuals are deliberately absent.
const PRIMARY_COVER_TYPES: &[&str] = &["box-2d", "box-2D", "box2d"];

/// Region preference used when the provider offers the same asset or text for several regions.
const REGION_PREFERENCE: &[&str] = &["wor", "us", "eu", "ss", "jp"];

/// Language preference for localized free text.
const LANGUAGE_PREFERENCE: &[&str] = &["en", "de"];

/// Image content types accepted for a cached cover.
pub const ACCEPTED_COVER_CONTENT_TYPES: &[&str] = &["image/png", "image/jpeg", "image/webp"];

/// Ceiling for a short normalized field (title, developer, publisher, genre, players, region).
///
/// The response body is already capped by the transport, but nothing else bounds one field, and a
/// provider-controlled string is stored verbatim in SQLite. These limits are far above any real
/// value and exist only so one malformed response cannot store megabytes per game.
const MAX_SHORT_FIELD_CHARS: usize = 512;

/// Ceiling for the one long normalized field.
const MAX_SYNOPSIS_CHARS: usize = 8_192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MalformedResponse;

/// The `response` object of a provider body.
pub fn response_object(body: &[u8]) -> Result<Value, MalformedResponse> {
    let root: Value = serde_json::from_slice(body).map_err(|_| MalformedResponse)?;
    root.get("response")
        .filter(|response| response.is_object())
        .cloned()
        .ok_or(MalformedResponse)
}

/// Names why a body could not be read as a provider envelope.
///
/// Diagnostic only — it never widens what is accepted. `MalformedResponse` deliberately carries no
/// detail so no call site can branch on it, but an operator staring at a failing library needs to
/// know whether the provider sent broken JSON, sent an error envelope, or sent something this
/// parser does not understand. Those demand completely different responses and the failure class
/// cannot tell them apart.
///
/// The returned text describes structure only: key names, JSON types, and the parser's own
/// position and message. It never includes a value from the body.
pub fn describe_envelope_failure(body: &[u8]) -> String {
    let root: Value = match serde_json::from_slice(body) {
        Ok(root) => root,
        Err(error) => {
            return format!(
                "body is not valid JSON: {} at line {} column {}",
                error.classify_text(),
                error.line(),
                error.column()
            )
        }
    };

    let Some(object) = root.as_object() else {
        return format!("root is {}, not an object", json_type_name(&root));
    };
    let keys: Vec<&str> = object.keys().map(String::as_str).collect();
    match object.get("response") {
        None => format!("root has no `response` key; keys are {keys:?}"),
        Some(response) => format!(
            "`response` is {}, not an object; root keys are {keys:?}",
            json_type_name(response)
        ),
    }
}

fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

trait ClassifyText {
    fn classify_text(&self) -> &'static str;
}

impl ClassifyText for serde_json::Error {
    fn classify_text(&self) -> &'static str {
        use serde_json::error::Category;
        match self.classify() {
            Category::Io => "read error",
            Category::Syntax => "syntax error",
            // The usual cause of this one is a byte sequence that is not valid UTF-8.
            Category::Data => "unexpected data or encoding",
            Category::Eof => "unexpected end of input (truncated)",
        }
    }
}

/// Extracts the dynamic quota snapshot.
///
/// Values are merged from the user profile and the server block so a guest profile, a member
/// profile, and a future relocation of the fields all work. Both observed spellings of the
/// per-minute maximum are accepted.
pub fn parse_quota(response: &Value) -> Option<ProviderQuotaSnapshot> {
    let sources = ["ssuser", "serveurs"]
        .into_iter()
        .filter_map(|key| response.get(key))
        .filter(|value| value.is_object())
        .collect::<Vec<_>>();
    if sources.is_empty() {
        return None;
    }

    let pick = |keys: &[&str]| -> Option<i64> {
        sources
            .iter()
            .find_map(|source| keys.iter().find_map(|key| number(source.get(*key))))
    };

    let snapshot = ProviderQuotaSnapshot {
        max_threads: pick(&["maxthreads"]),
        // The documentation spells this both ways; the observed JSON used the first.
        max_requests_per_minute: pick(&["maxrequestspermin", "maxrequestsperdmin"]),
        max_requests_per_day: pick(&["maxrequestsperday"]),
        max_negative_requests_per_day: pick(&["maxrequestskoperday"]),
        requests_today: pick(&["requeststoday"]),
        negative_requests_today: pick(&["requestskotoday"]),
    };
    (snapshot != ProviderQuotaSnapshot::default()).then_some(snapshot)
}

/// The optional account name the provider echoes back for a logged-in profile.
pub fn parse_account_name(response: &Value) -> Option<String> {
    text(response.pointer("/ssuser/id"))
}

/// Parses one game object into the provider-neutral record.
pub fn parse_game(response: &Value) -> Result<ProviderGameRecord, MalformedResponse> {
    let game = response
        .get("jeu")
        .filter(|game| game.is_object())
        .ok_or(MalformedResponse)?;
    let provider_game_id = text(game.get("id")).ok_or(MalformedResponse)?;

    let roms: Vec<ProviderRomRecord> = game
        .get("roms")
        .and_then(Value::as_array)
        .map(|roms| roms.iter().map(parse_rom).collect())
        .unwrap_or_default();
    let matched_rom = game.get("rom").filter(|rom| rom.is_object()).map(parse_rom);

    // The matched record is also listed so collision checks see the complete set exactly once.
    let mut all_roms = roms;
    if let Some(matched) = matched_rom.as_ref() {
        if !all_roms
            .iter()
            .any(|rom| rom.provider_rom_id == matched.provider_rom_id)
        {
            all_roms.push(matched.clone());
        }
    }

    let title = bound(
        regional_text(game.get("noms")).unwrap_or_else(|| provider_game_id.clone()),
        MAX_SHORT_FIELD_CHARS,
    );
    let metadata = NormalizedMetadata {
        sort_title: Some(bound(derive_sort_title(&title), MAX_SHORT_FIELD_CHARS)),
        title,
        synopsis: localized_text(game.get("synopsis"))
            .map(|value| bound(value, MAX_SYNOPSIS_CHARS)),
        release_date: regional_text(game.get("dates"))
            .map(|value| bound(value, MAX_SHORT_FIELD_CHARS)),
        developer: text(game.pointer("/developpeur/text"))
            .or_else(|| text(game.get("developpeur")))
            .map(|value| bound(value, MAX_SHORT_FIELD_CHARS)),
        publisher: text(game.pointer("/editeur/text"))
            .or_else(|| text(game.get("editeur")))
            .map(|value| bound(value, MAX_SHORT_FIELD_CHARS)),
        genre: parse_genres(game.get("genres")).map(|value| bound(value, MAX_SHORT_FIELD_CHARS)),
        players: text(game.pointer("/joueurs/text"))
            .or_else(|| text(game.get("joueurs")))
            .map(|value| bound(value, MAX_SHORT_FIELD_CHARS)),
        region: matched_rom
            .as_ref()
            .and_then(|_| text(game.pointer("/rom/romregions")))
            .or_else(|| first_region(game.get("noms")))
            .map(|value| bound(value, MAX_SHORT_FIELD_CHARS)),
    };

    Ok(ProviderGameRecord {
        provider_game_id,
        provider_rom_id: text(game.get("romid")),
        matched_rom,
        roms: all_roms,
        metadata,
        source_credit: parse_source_credit(game),
        primary_cover: select_primary_cover(game.get("medias")),
    })
}

/// Parses heuristic search results, preserving provider order without treating it as a score.
pub fn parse_candidates(response: &Value) -> Result<Vec<ProviderCandidate>, MalformedResponse> {
    let games = match response.get("jeux") {
        Some(Value::Array(games)) => games.clone(),
        // A single-result body and an absent-results body are both valid provider answers.
        Some(game) if game.is_object() => vec![game.clone()],
        None | Some(Value::Null) => Vec::new(),
        Some(_) => return Err(MalformedResponse),
    };

    Ok(games
        .iter()
        .filter_map(|game| {
            let provider_game_id = text(game.get("id"))?;
            let title = bound(
                regional_text(game.get("noms")).unwrap_or_else(|| provider_game_id.clone()),
                MAX_SHORT_FIELD_CHARS,
            );
            Some(ProviderCandidate {
                provider_game_id,
                title,
                release_date: regional_text(game.get("dates"))
                    .map(|value| bound(value, MAX_SHORT_FIELD_CHARS)),
            })
        })
        .collect())
}

fn parse_rom(rom: &Value) -> ProviderRomRecord {
    ProviderRomRecord {
        provider_rom_id: text(rom.get("id")),
        filename: text(rom.get("romfilename")),
        size_bytes: number(rom.get("romsize")).and_then(|size| u64::try_from(size).ok()),
        crc32: text(rom.get("romcrc")),
        md5: text(rom.get("rommd5")),
        sha1: text(rom.get("romsha1")),
        support_number: number(rom.get("romnumsupport")),
        support_count: number(rom.get("romtotalsupport")),
    }
}

/// Chooses the single primary front cover.
///
/// Selection is explicit and ordered so the same response always yields the same asset: front box
/// art only, preferred region first, and the provider's own checksums retained for refresh
/// comparison.
pub fn select_primary_cover(medias: Option<&Value>) -> Option<ProviderCoverDescriptor> {
    let medias = medias?.as_array()?;
    let mut best: Option<(usize, usize, &Value)> = None;

    for media in medias {
        let Some(media_type) = text(media.get("type")) else {
            continue;
        };
        let Some(type_rank) = PRIMARY_COVER_TYPES
            .iter()
            .position(|candidate| candidate.eq_ignore_ascii_case(&media_type))
        else {
            continue;
        };
        // Reject media attached to something other than the game itself.
        if let Some(parent) = text(media.get("parent")) {
            if !parent.eq_ignore_ascii_case("jeu") {
                continue;
            }
        }
        if text(media.get("url")).is_none() {
            continue;
        }
        let region_rank = region_rank(text(media.get("region")).as_deref());
        if best.is_none_or(|(best_type, best_region, _)| {
            (type_rank, region_rank) < (best_type, best_region)
        }) {
            best = Some((type_rank, region_rank, media));
        }
    }

    let (_, _, media) = best?;
    Some(ProviderCoverDescriptor {
        provider_media_type: text(media.get("type"))?,
        region: text(media.get("region")),
        crc32: text(media.get("crc")),
        md5: text(media.get("md5")),
        sha1: text(media.get("sha1")),
        source_credit: text(media.get("source")),
        locator: ProviderMediaLocator::new(text(media.get("url"))?),
    })
}

/// Provider-reported source/category credit, preserved so M6 can render attribution.
///
/// The provider attaches credits per entry rather than per game, so the first credit found across
/// the title and synopsis lists is kept. It is stored as provenance, never as a legal conclusion.
fn parse_source_credit(game: &Value) -> Option<String> {
    text(game.get("source"))
        .or_else(|| first_entry_source(game.get("noms")))
        .or_else(|| first_entry_source(game.get("synopsis")))
}

fn first_entry_source(values: Option<&Value>) -> Option<String> {
    match values? {
        Value::Array(entries) => entries.iter().find_map(|entry| text(entry.get("source"))),
        entry if entry.is_object() => text(entry.get("source")),
        _ => None,
    }
}

fn parse_genres(genres: Option<&Value>) -> Option<String> {
    let genres = genres?.as_array()?;
    let names: Vec<String> = genres
        .iter()
        .filter_map(|genre| localized_text(genre.get("noms")).or_else(|| text(genre.get("text"))))
        .collect();
    (!names.is_empty()).then(|| names.join(", "))
}

/// Picks a value from a `[{ region, text }]` list using the region preference.
fn regional_text(values: Option<&Value>) -> Option<String> {
    entry_by_rank(values, "region", region_rank).and_then(|entry| text(entry.get("text")))
}

fn first_region(values: Option<&Value>) -> Option<String> {
    entry_by_rank(values, "region", region_rank).and_then(|entry| text(entry.get("region")))
}

/// Picks a value from a `[{ langue, text }]` list using the language preference.
fn localized_text(values: Option<&Value>) -> Option<String> {
    entry_by_rank(values, "langue", language_rank).and_then(|entry| text(entry.get("text")))
}

fn entry_by_rank<'a>(
    values: Option<&'a Value>,
    key: &str,
    rank: impl Fn(Option<&str>) -> usize,
) -> Option<&'a Value> {
    match values? {
        Value::Array(entries) => entries
            .iter()
            .filter(|entry| entry.get("text").is_some())
            .min_by_key(|entry| rank(entry.get(key).and_then(Value::as_str))),
        entry if entry.is_object() => Some(entry),
        _ => None,
    }
}

fn region_rank(region: Option<&str>) -> usize {
    ranked(region, REGION_PREFERENCE)
}

fn language_rank(language: Option<&str>) -> usize {
    ranked(language, LANGUAGE_PREFERENCE)
}

fn ranked(value: Option<&str>, preference: &[&str]) -> usize {
    value
        .and_then(|value| {
            preference
                .iter()
                .position(|candidate| candidate.eq_ignore_ascii_case(value))
        })
        .unwrap_or(preference.len())
}

/// Reads a number that the beta schema may encode as a JSON number or a string.
fn number(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|value| value as i64)),
        Value::String(text) => text.trim().parse::<i64>().ok(),
        _ => None,
    }
}

/// Reads a non-empty trimmed string, accepting a numeric encoding.
fn text(value: Option<&Value>) -> Option<String> {
    let value = match value? {
        Value::String(text) => text.trim().to_owned(),
        Value::Number(number) => number.to_string(),
        _ => return None,
    };
    (!value.is_empty()).then_some(value)
}

/// Truncates a provider-controlled string on a character boundary.
fn bound(value: String, maximum_chars: usize) -> String {
    match value.char_indices().nth(maximum_chars) {
        Some((offset, _)) => value[..offset].to_owned(),
        None => value,
    }
}

/// Moves a leading English article to the end so M6 can sort naturally.
///
/// This is a presentation normalization of the provider's own title, not invented metadata.
fn derive_sort_title(title: &str) -> String {
    for article in ["The ", "A ", "An "] {
        if let Some(remainder) = title.strip_prefix(article) {
            return format!("{}, {}", remainder.trim(), article.trim());
        }
    }
    title.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic fixture. It mirrors the documented response *shape* only: no real provider payload,
    /// no real credentials, no credential-bearing URL, and no provider artwork.
    const GAME_FIXTURE: &str = r#"{
      "header": { "APIversion": "2.0" },
      "response": {
        "serveurs": {
          "cpu1": "12",
          "maxthreads": "1",
          "maxdownloadspeed": "128",
          "maxrequestspermin": "3072",
          "maxrequestsperday": "10000",
          "maxrequestskoperday": "1000",
          "requeststoday": "7",
          "requestskotoday": "2",
          "unknown_future_field": "ignored"
        },
        "jeu": {
          "id": "3",
          "romid": "77",
          "noms": [
            { "region": "jp", "text": "Rendai no Rei" },
            { "region": "us", "text": "The Example Quest", "source": "Example Contributor" }
          ],
          "synopsis": [
            { "langue": "fr", "text": "Synopsis francais." },
            { "langue": "en", "text": "A synthetic synopsis." }
          ],
          "dates": [
            { "region": "jp", "text": "1990-03-01" },
            { "region": "us", "text": "1990-09-01" }
          ],
          "developpeur": { "id": "5", "text": "Example Studio" },
          "editeur": { "id": "6", "text": "Example Publisher" },
          "genres": [
            { "id": "10", "noms": [ { "langue": "en", "text": "Action" } ] },
            { "id": "11", "noms": [ { "langue": "en", "text": "Platform" } ] }
          ],
          "joueurs": { "text": "1-2" },
          "rom": {
            "id": "101",
            "romfilename": "Example Quest (USA).sfc",
            "romsize": "524288",
            "romcrc": "AABBCCDD",
            "rommd5": "d41d8cd98f00b204e9800998ecf8427e",
            "romsha1": "da39a3ee5e6b4b0d3255bfef95601890afd80709",
            "romregions": "us",
            "romnumsupport": "1",
            "romtotalsupport": "1"
          },
          "roms": [
            {
              "id": "102",
              "romfilename": "Example Quest (Japan).sfc",
              "romsize": 524288,
              "romcrc": "11223344"
            }
          ],
          "medias": [
            { "type": "ss", "parent": "jeu", "url": "https://provider.invalid/media/ss", "region": "us" },
            { "type": "box-2D", "parent": "jeu", "url": "https://provider.invalid/media/cover-jp", "region": "jp", "crc": "0F0F0F0F" },
            { "type": "box-2D", "parent": "jeu", "url": "https://provider.invalid/media/cover-us", "region": "us", "crc": "1A2B3C4D", "md5": "d41d8cd98f00b204e9800998ecf8427e", "source": "Example Media Source" },
            { "type": "wheel", "parent": "jeu", "url": "https://provider.invalid/media/wheel", "region": "wor" }
          ]
        }
      }
    }"#;

    fn fixture_response() -> Value {
        response_object(GAME_FIXTURE.as_bytes()).expect("fixture should parse")
    }

    #[test]
    fn quota_is_read_from_the_provider_response() {
        let quota = parse_quota(&fixture_response()).expect("fixture reports quota");

        assert_eq!(quota.max_threads, Some(1));
        assert_eq!(quota.max_requests_per_minute, Some(3072));
        assert_eq!(quota.max_requests_per_day, Some(10_000));
        assert_eq!(quota.max_negative_requests_per_day, Some(1_000));
        assert_eq!(quota.requests_today, Some(7));
        assert_eq!(quota.negative_requests_today, Some(2));
    }

    #[test]
    fn both_documented_minute_quota_spellings_are_accepted() {
        let body = br#"{"response":{"ssuser":{"maxrequestsperdmin":"600","maxthreads":2}}}"#;
        let quota = parse_quota(&response_object(body).unwrap()).expect("quota should parse");

        assert_eq!(quota.max_requests_per_minute, Some(600));
        assert_eq!(quota.max_threads, Some(2));
        assert_eq!(quota.max_requests_per_day, None);
    }

    #[test]
    fn a_response_without_quota_information_reports_none() {
        let body = br#"{"response":{"jeu":{"id":"3"}}}"#;
        assert_eq!(parse_quota(&response_object(body).unwrap()), None);
    }

    #[test]
    fn game_parsing_normalizes_the_documented_field_set() {
        let record = parse_game(&fixture_response()).expect("fixture game should parse");

        assert_eq!(record.provider_game_id, "3");
        assert_eq!(record.provider_rom_id.as_deref(), Some("77"));
        assert_eq!(record.metadata.title, "The Example Quest");
        assert_eq!(
            record.metadata.sort_title.as_deref(),
            Some("Example Quest, The")
        );
        assert_eq!(
            record.metadata.synopsis.as_deref(),
            Some("A synthetic synopsis.")
        );
        assert_eq!(record.metadata.release_date.as_deref(), Some("1990-09-01"));
        assert_eq!(record.metadata.developer.as_deref(), Some("Example Studio"));
        assert_eq!(
            record.metadata.publisher.as_deref(),
            Some("Example Publisher")
        );
        assert_eq!(record.metadata.genre.as_deref(), Some("Action, Platform"));
        assert_eq!(record.metadata.players.as_deref(), Some("1-2"));
        assert_eq!(record.metadata.region.as_deref(), Some("us"));
        assert_eq!(record.source_credit.as_deref(), Some("Example Contributor"));

        let matched = record.matched_rom.expect("fixture has a matched record");
        assert_eq!(matched.provider_rom_id.as_deref(), Some("101"));
        assert_eq!(matched.size_bytes, Some(524_288));
        assert_eq!(
            matched.sha1.as_deref(),
            Some("da39a3ee5e6b4b0d3255bfef95601890afd80709")
        );
        assert_eq!(matched.support_number, Some(1));
        assert_eq!(matched.support_count, Some(1));
        assert_eq!(
            record.roms.len(),
            2,
            "the matched record and the listed record are both available for conflict checks"
        );
    }

    #[test]
    fn the_selected_cover_is_the_preferred_region_front_box_art_only() {
        let record = parse_game(&fixture_response()).expect("fixture game should parse");
        let cover = record.primary_cover.expect("fixture offers box art");

        assert_eq!(cover.provider_media_type, "box-2D");
        assert_eq!(cover.region.as_deref(), Some("us"));
        assert_eq!(cover.crc32.as_deref(), Some("1A2B3C4D"));
        assert_eq!(cover.source_credit.as_deref(), Some("Example Media Source"));
        assert_eq!(
            cover.locator.expose(),
            "https://provider.invalid/media/cover-us"
        );
    }

    #[test]
    fn non_cover_media_categories_are_never_selected() {
        let body = br#"{"response":{"jeu":{"id":"3","medias":[
            {"type":"ss","parent":"jeu","url":"https://provider.invalid/s"},
            {"type":"wheel","parent":"jeu","url":"https://provider.invalid/w"},
            {"type":"video","parent":"jeu","url":"https://provider.invalid/v"},
            {"type":"manuel","parent":"jeu","url":"https://provider.invalid/m"},
            {"type":"fanart","parent":"jeu","url":"https://provider.invalid/f"},
            {"type":"box-2D-back","parent":"jeu","url":"https://provider.invalid/b"}
        ]}}}"#;
        let record = parse_game(&response_object(body).unwrap()).expect("game should parse");

        assert!(record.primary_cover.is_none());
    }

    #[test]
    fn cover_media_attached_to_another_parent_is_ignored() {
        let body = br#"{"response":{"jeu":{"id":"3","medias":[
            {"type":"box-2D","parent":"systeme","url":"https://provider.invalid/system-box"}
        ]}}}"#;
        let record = parse_game(&response_object(body).unwrap()).expect("game should parse");

        assert!(record.primary_cover.is_none());
    }

    #[test]
    fn missing_optional_fields_are_tolerated() {
        let body = br#"{"response":{"jeu":{"id":"42"}}}"#;
        let record = parse_game(&response_object(body).unwrap()).expect("a minimal game is valid");

        assert_eq!(record.provider_game_id, "42");
        assert_eq!(record.metadata.title, "42");
        assert_eq!(record.metadata.synopsis, None);
        assert_eq!(record.metadata.developer, None);
        assert!(record.matched_rom.is_none());
        assert!(record.roms.is_empty());
        assert!(record.primary_cover.is_none());
    }

    #[test]
    fn unknown_additional_fields_do_not_break_parsing() {
        let body = br#"{"response":{"jeu":{"id":"42","brand_new_field":{"nested":[1,2,3]},
            "rom":{"id":"9","romsize":"10","romcrc":"AA","future":"x"}},"future_block":true}}"#;
        let record = parse_game(&response_object(body).unwrap()).expect("game should parse");

        assert_eq!(record.provider_game_id, "42");
        assert_eq!(record.matched_rom.unwrap().size_bytes, Some(10));
    }

    #[test]
    fn an_envelope_failure_is_described_precisely_and_leaks_no_values() {
        // Broken JSON names the parser's own position.
        let truncated = describe_envelope_failure(br#"{"header":{"a":1"#);
        assert!(truncated.contains("not valid JSON"), "{truncated}");
        assert!(truncated.contains("line"), "{truncated}");

        // A body that parses but is not an envelope names the structure it found instead.
        let wrong_shape =
            describe_envelope_failure(br#"{"header":{},"response":"nope","secret":"hunter2"}"#);
        assert!(
            wrong_shape.contains("`response` is a string"),
            "{wrong_shape}"
        );
        assert!(wrong_shape.contains("header"), "{wrong_shape}");
        // Key names are structure; values never are.
        assert!(!wrong_shape.contains("hunter2"), "{wrong_shape}");
        assert!(!wrong_shape.contains("nope"), "{wrong_shape}");

        let missing = describe_envelope_failure(br#"{"header":{}}"#);
        assert!(missing.contains("no `response` key"), "{missing}");

        let not_object = describe_envelope_failure(b"[1,2,3]");
        assert!(not_object.contains("root is an array"), "{not_object}");

        // Bytes that are not valid UTF-8 are a real provider failure mode, and must be named as
        // an encoding problem rather than reported as a structural one.
        let latin1 = describe_envelope_failure(b"{\"response\":{\"nom\":\"caf\xe9\"}}");
        assert!(latin1.contains("not valid JSON"), "{latin1}");
    }
    #[test]
    fn malformed_bodies_are_rejected_rather_than_producing_empty_metadata() {
        assert_eq!(response_object(b"not json"), Err(MalformedResponse));
        assert_eq!(response_object(b"[]"), Err(MalformedResponse));
        assert_eq!(
            response_object(br#"{"response":"text"}"#),
            Err(MalformedResponse)
        );

        let no_game = response_object(br#"{"response":{}}"#).unwrap();
        assert_eq!(parse_game(&no_game), Err(MalformedResponse));

        let wrong_type = response_object(br#"{"response":{"jeu":[]}}"#).unwrap();
        assert_eq!(parse_game(&wrong_type), Err(MalformedResponse));

        let no_identifier = response_object(br#"{"response":{"jeu":{"noms":[]}}}"#).unwrap();
        assert_eq!(parse_game(&no_identifier), Err(MalformedResponse));
    }

    #[test]
    fn candidate_search_preserves_provider_order_without_a_score() {
        let body = br#"{"response":{"jeux":[
            {"id":"11","noms":[{"region":"us","text":"Second Best"}],"dates":[{"region":"us","text":"1992"}]},
            {"id":"12","noms":[{"region":"us","text":"Another Result"}]},
            {"noms":[{"region":"us","text":"No Identifier"}]}
        ]}}"#;
        let candidates = parse_candidates(&response_object(body).unwrap()).expect("should parse");

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].provider_game_id, "11");
        assert_eq!(candidates[0].title, "Second Best");
        assert_eq!(candidates[0].release_date.as_deref(), Some("1992"));
        assert_eq!(candidates[1].provider_game_id, "12");
        assert_eq!(candidates[1].release_date, None);
    }

    #[test]
    fn candidate_search_accepts_empty_and_single_result_bodies() {
        let empty = response_object(br#"{"response":{}}"#).unwrap();
        assert_eq!(parse_candidates(&empty), Ok(Vec::new()));

        let single = response_object(br#"{"response":{"jeux":{"id":"5"}}}"#).unwrap();
        let candidates = parse_candidates(&single).expect("should parse");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].title, "5");

        let malformed = response_object(br#"{"response":{"jeux":"text"}}"#).unwrap();
        assert_eq!(parse_candidates(&malformed), Err(MalformedResponse));
    }

    #[test]
    fn sort_titles_move_a_leading_article() {
        assert_eq!(derive_sort_title("The Example Quest"), "Example Quest, The");
        assert_eq!(derive_sort_title("A Short Hike"), "Short Hike, A");
        assert_eq!(derive_sort_title("An Odd Title"), "Odd Title, An");
        assert_eq!(derive_sort_title("Example Quest"), "Example Quest");
    }
}
