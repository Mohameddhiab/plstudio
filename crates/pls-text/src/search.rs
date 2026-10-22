//! Recherche naïve par sous-chaîne de caractères.
//!
//! Suffisant pour Phase 1 (`editor.find`) ; une recherche par regex ou par
//! ropey segments arrivera en Phase 2. Les tranches retournées sont des
//! CharSpans stabilisables uniquement à l'instant du calcul.

use ropey::Rope;

use crate::{CharSpan, ErrorCode, PlsError, Result};

/// Trouve la première occurrence de `needle` à partir de `from_char`.
pub fn find_next(rope: &Rope, needle: &str, from_char: usize) -> Result<Option<CharSpan>> {
    if needle.is_empty() {
        return Err(
            PlsError::app(ErrorCode::INVALID_PARAMS, "the search needle is empty")
                .with_remediation("provide a non-empty needle"),
        );
    }

    let len = rope.len_chars();
    if from_char >= len {
        return Ok(None);
    }

    let haystack = rope.slice(from_char..len).to_string();
    let byte_offset = haystack.find(needle);
    let Some(byte_offset) = byte_offset else {
        return Ok(None);
    };

    let start = from_char + haystack[..byte_offset].chars().count();
    let width = needle.chars().count();

    Ok(Some(CharSpan {
        start,
        end: start + width,
    }))
}

/// Trouve toutes les occurrences, sans chevauchement, dans l'ordre avant.
pub fn find_all(rope: &Rope, needle: &str) -> Result<Vec<CharSpan>> {
    let mut spans = Vec::new();
    let mut from = 0_usize;

    loop {
        let Some(span) = find_next(rope, needle, from)? else {
            return Ok(spans);
        };

        from = span.end;
        spans.push(span);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rope(text: &str) -> Rope {
        Rope::from_str(text)
    }

    #[test]
    fn finds_forward_from_a_offset() {
        let rope = rope("ababab");
        let span = find_next(&rope, "ab", 1).expect("no error").expect("found");
        assert_eq!(span, CharSpan { start: 2, end: 4 });
        assert!(find_next(&rope, "ab", 5).expect("no error").is_none());
    }

    #[test]
    fn counts_multibyte_needles_correctly() {
        let rope = rope("héhéhé");
        let span = find_next(&rope, "é", 0).expect("no error").expect("found");
        assert_eq!(span, CharSpan { start: 1, end: 2 });
    }

    #[test]
    fn find_all_returns_non_overlapping_occurrences() {
        let rope = rope("aaaaaa");
        let spans = find_all(&rope, "aa").expect("no error");
        assert_eq!(spans.len(), 3);

        assert!(find_all(&rope, "z").expect("no error").is_empty());
    }

    #[test]
    fn rejects_an_empty_needle() {
        let rope = rope("aaa");
        assert!(find_next(&rope, "", 0).is_err());
        assert!(find_all(&rope, "").is_err());
    }
}
