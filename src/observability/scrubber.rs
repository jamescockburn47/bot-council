//! PII scrubber for Sentry events and breadcrumbs.
//!
//! Any field whose key matches [`SENSITIVE_KEY`] is replaced with the literal
//! string `"[redacted]"` before the event is sent. This catches tracing
//! structured fields, request headers, cookies, and tags that name bearer
//! tokens, JWTs, AES keys, or bot ciphertext.

use regex::Regex;
use sentry::protocol::{Breadcrumb, Event};
use std::sync::OnceLock;

/// Keys matching this pattern (case-insensitive substring) are redacted.
const SENSITIVE_PATTERN: &str =
    r"(?i)(token|authorization|ciphertext|jwt|cookie|bearer|api[_-]?key|secret|password)";

fn sensitive() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(SENSITIVE_PATTERN).expect("valid regex"))
}

/// Sentry `before_send` hook. Redacts sensitive fields in place.
pub fn before_send(mut event: Event<'static>) -> Option<Event<'static>> {
    redact_extra(&mut event.extra);
    redact_tags(&mut event.tags);
    if let Some(req) = event.request.as_mut() {
        redact_extra(&mut req.headers);
        redact_extra(&mut req.env);
        if let Some(q) = req.query_string.as_mut() {
            if sensitive().is_match(q) {
                *q = "[redacted]".to_string();
            }
        }
        req.cookies = None;
    }
    Some(event)
}

/// Sentry `before_breadcrumb` hook. Redacts sensitive fields on breadcrumbs.
pub fn before_breadcrumb(mut bc: Breadcrumb) -> Option<Breadcrumb> {
    redact_extra(&mut bc.data);
    if let Some(msg) = bc.message.as_mut() {
        // Coarse protection for accidental bearer tokens pasted into log
        // messages. Replace the whole message if it looks like it contains
        // a Bearer header.
        if sensitive().is_match(msg) && msg.len() > 40 {
            *msg = "[redacted message]".to_string();
        }
    }
    Some(bc)
}

fn redact_extra<V>(map: &mut std::collections::BTreeMap<String, V>)
where
    V: From<String>,
{
    let re = sensitive();
    for (k, v) in map.iter_mut() {
        if re.is_match(k) {
            *v = V::from("[redacted]".to_string());
        }
    }
}

fn redact_tags(map: &mut std::collections::BTreeMap<String, String>) {
    let re = sensitive();
    for (k, v) in map.iter_mut() {
        if re.is_match(k) {
            *v = "[redacted]".to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentry::protocol::{Map, Request, Value};

    fn event_with_extras(pairs: &[(&str, &str)]) -> Event<'static> {
        let mut ev = Event::default();
        for (k, v) in pairs {
            ev.extra.insert(k.to_string(), Value::String(v.to_string()));
        }
        ev
    }

    #[test]
    fn redacts_bearer_token_in_extra() {
        let ev = event_with_extras(&[
            ("authorization", "Bearer eyJhbGciOiJSUzI1..."),
            ("debate_id", "abc-123"),
        ]);
        let out = before_send(ev).unwrap();
        assert_eq!(out.extra.get("authorization").unwrap(), "[redacted]");
        assert_eq!(out.extra.get("debate_id").unwrap(), "abc-123");
    }

    #[test]
    fn redacts_token_key_variants() {
        let mut ev = Event::default();
        for k in [
            "api_key",
            "API-Key",
            "bot_token_key",
            "token_ciphertext",
            "password",
        ] {
            ev.extra
                .insert(k.to_string(), Value::String("secret".to_string()));
        }
        let out = before_send(ev).unwrap();
        for k in [
            "api_key",
            "API-Key",
            "bot_token_key",
            "token_ciphertext",
            "password",
        ] {
            assert_eq!(
                out.extra.get(k).unwrap(),
                "[redacted]",
                "key {} not redacted",
                k
            );
        }
    }

    #[test]
    fn redacts_tags_and_cookies() {
        let mut ev = Event::default();
        ev.tags.insert("jwt".to_string(), "eyJh...".to_string());
        ev.tags.insert("role".to_string(), "admin".to_string());
        let mut req = Request::default();
        req.cookies = Some("session=abc".to_string());
        req.headers = Map::new();
        req.headers
            .insert("Authorization".to_string(), "Bearer xxx".to_string());
        ev.request = Some(req);

        let out = before_send(ev).unwrap();
        assert_eq!(out.tags.get("jwt").unwrap(), "[redacted]");
        assert_eq!(out.tags.get("role").unwrap(), "admin");
        let r = out.request.unwrap();
        assert!(r.cookies.is_none());
        assert_eq!(r.headers.get("Authorization").unwrap(), "[redacted]");
    }

    #[test]
    fn scrubs_long_sensitive_breadcrumb_message() {
        let mut bc = Breadcrumb::default();
        bc.message = Some(
            "attempt with authorization bearer eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.xxxx".into(),
        );
        let out = before_breadcrumb(bc).unwrap();
        assert_eq!(out.message.unwrap(), "[redacted message]");
    }

    #[test]
    fn leaves_benign_breadcrumb_message() {
        let mut bc = Breadcrumb::default();
        bc.message = Some("debate started".into());
        let out = before_breadcrumb(bc).unwrap();
        assert_eq!(out.message.unwrap(), "debate started");
    }
}
