use crate::config::ModelsConfig;
use serde::{Deserialize, Serialize};

/// The single most-divergent claim picked from R1 responses. Injected into
/// the R3 crux prompt so every bot engages the same point head-on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CruxSelection {
    pub claim: String,
    pub source_pseudonym: String,
    pub source_quote: String,
}

/// One participant's R0 + R1 texts, fed to the crux selector.
#[derive(Debug, Clone, Serialize)]
pub struct R1Entry {
    pub pseudonym: String,
    pub r0: String,
    pub r1: String,
}

/// Failure modes for crux selection.
///
/// `NoValidCandidate` is the terminal state after one retry: either
/// MiniMax produced malformed JSON both times, or the returned
/// `source_quote` failed substring verification both times. The caller
/// should fall back to the legacy cross-examination R3 format.
#[derive(Debug, Clone)]
pub enum CruxError {
    MinimaxFailed(String),
    NoValidCandidate,
}

/// Pick the single most-divergent claim from R1 responses.
///
/// One MiniMax call with a strict JSON schema. If the returned
/// `source_quote` is not a verbatim substring of the named bot's R1
/// text, re-prompt once with the failure reason appended. If the second
/// attempt also fails, return `Err(CruxError::NoValidCandidate)` —
/// the caller should fall back to the legacy cross-examination R3
/// format rather than synthesising a crux.
pub async fn select_crux(
    models_config: &ModelsConfig,
    topic: &str,
    r1_entries: &[R1Entry],
) -> Result<CruxSelection, CruxError> {
    let entries_json =
        serde_json::to_string(r1_entries).map_err(|e| CruxError::MinimaxFailed(e.to_string()))?;

    let base_prompt = format!(
        "{n} participants wrote R0 and R1 responses on the topic: {topic}\n\n\
         The R1 responses each identified the strongest opposing argument. \
         Identify the single claim across these responses that creates the \
         widest, sharpest disagreement — the one where participants most \
         visibly clash.\n\n\
         R1 responses (JSON):\n{entries}\n\n\
         Return EXACTLY this JSON, no prose outside it:\n\
         {{\"claim\": \"<1-sentence restatement of the claim>\", \
           \"source_pseudonym\": \"<pseudonym of who first stated it>\", \
           \"source_quote\": \"<verbatim substring of that bot's R1 text>\"}}",
        n = r1_entries.len(),
        topic = topic,
        entries = entries_json,
    );

    for attempt in 0..2u8 {
        let prompt = if attempt == 0 {
            base_prompt.clone()
        } else {
            format!(
                "{base_prompt}\n\n\
                 Your previous attempt failed verification: the source_quote \
                 was not a verbatim substring of the named bot's R1 text. \
                 Use exact text from that bot's R1 response."
            )
        };
        let raw = crate::analyser::call_minimax(models_config, &prompt)
            .await
            .map_err(CruxError::MinimaxFailed)?;

        let parsed: CruxSelection = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => continue, // malformed → retry
        };

        if let Some(entry) = r1_entries
            .iter()
            .find(|e| e.pseudonym == parsed.source_pseudonym)
        {
            if crate::extractor::verify::quote_is_substring_of(&parsed.source_quote, &entry.r1) {
                return Ok(parsed);
            }
        }
        // fall through to retry
    }

    Err(CruxError::NoValidCandidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crux_selection_serialises_round_trip() {
        let c = CruxSelection {
            claim: "X is Y".into(),
            source_pseudonym: "Agent A".into(),
            source_quote: "X is Y because Z".into(),
        };
        let s = serde_json::to_string(&c).unwrap();
        let round: CruxSelection = serde_json::from_str(&s).unwrap();
        assert_eq!(round.claim, c.claim);
        assert_eq!(round.source_pseudonym, c.source_pseudonym);
        assert_eq!(round.source_quote, c.source_quote);
    }

    #[test]
    fn r1_entry_serialises_for_prompt_injection() {
        let e = R1Entry {
            pseudonym: "Agent A".into(),
            r0: "round 0 text".into(),
            r1: "round 1 text".into(),
        };
        let s = serde_json::to_string(&e).unwrap();
        assert!(s.contains("round 0 text"));
        assert!(s.contains("round 1 text"));
    }
}
