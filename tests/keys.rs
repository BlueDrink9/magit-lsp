//! Key notation round trips and rejections (§3.8).

use magit_lsp::keys::KeyChord;

#[test]
fn round_trips_every_canonical_form() {
    for case in [
        "s",
        "S",
        "?",
        "%",
        "$",
        "!",
        "-",
        "=",
        "Enter",
        "ctrl+Enter",
        "Escape",
        "Delete",
        "shift+Tab",
        "alt+ArrowDown",
        "ctrl+c",
        "] ]",
        "g r",
        "ctrl+k ctrl+s",
        "z a",
        "Space",
        "Plus",
        "ctrl+Plus",
        "F5",
        "meta+PageUp",
        "ctrl+shift+c",
    ] {
        let parsed = KeyChord::parse(case).unwrap_or_else(|e| panic!("{case}: {e}"));
        assert_eq!(parsed.to_canonical(), case);
    }
}

#[test]
fn rejects_invalid_input() {
    for case in ["", "  ", "shift+s", "shift+S", "ctrl+", "+", "ctrl+nope", "NotAKey"] {
        assert!(KeyChord::parse(case).is_err(), "expected rejection: {case:?}");
    }
}
