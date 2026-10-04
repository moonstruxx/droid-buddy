//! Pure SysEx payload builder for the MIDI upload (`midi-upload` task 1.1).
//!
//! Input is rendered patch text: callers serialize `App.patch` via the
//! existing lossless writer (`Patch::write_to_ini` / `render_ini` in
//! `crate::patch`) and pass the text here — serialization is NOT
//! reimplemented. Output is the framed SysEx byte string ready to hand to
//! `amidi -s` / `sendmidi syf`.
//!
//! The stripping rules are a line-by-line port of the reference `droidpatch`
//! script (`utilities/sysex/linux/droidpatch`, the `sed`/`tr` pipeline):
//!
//! ```text
//! sed -e 's/^\([^"]*\)#.*/\1/' \
//!     -e 's/^[[:space:]]*//' \
//!     -e 's/^\([^=[:space:]]*\)[[:space:]]=[[:space:]]/\1=/' \
//!     -e '/^[^"]*$/s/[[:space:]]//g' \
//!     -e '/^[^"]*$/s/#.*//' \
//!     | tr -c -d '\n-~'
//! ```
//!
//! In words, per line: strip a `#` comment unless a `"` precedes the `#`
//! (careful with `#`/spaces inside quoted strings); strip leading
//! whitespace; collapse one padding space on each side of the first `=`;
//! on lines without any `"` remove all remaining whitespace; then drop
//! every byte outside `0x0A..=0x7E` (GNU `tr` parses `'\n-~'` as the range
//! newline–`~`, so spaces inside quoted strings survive while tabs,
//! control characters, and non-ASCII bytes are deleted — MIDI SysEx is
//! 7-bit). The cleaned text is wrapped in `F0 00 66 66 50 … F7`.
//!
//! No process, filesystem, or UI dependency: plain functions over
//! strings/bytes, like `crate::diff`.

/// SysEx header written by the reference script (`\0360\0000\0146\0146P`):
/// SOX + DROID manufacturer id + `P` (patch) command.
pub const SYSEX_HEADER: [u8; 5] = [0xF0, 0x00, 0x66, 0x66, 0x50];

/// SysEx end byte (`\0367`).
pub const SYSEX_FOOTER: u8 = 0xF7;

/// First/last byte value the `tr -c -d '\n-~'` stage keeps (inclusive).
const KEPT_FIRST: char = '\u{0A}';
const KEPT_LAST: char = '\u{7E}';

fn is_space_byte(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | 0x0B | 0x0C | b'\r')
}

/// Strip one rendered patch line per the `droidpatch` `sed` rules.
///
/// Faithful quirks, ported as-is: the comment strip fires when the text
/// before the first `#` holds no `"` (so `x = 1 # "q"` still strips, while
/// `header = "a # b"` keeps the `#`); the `=` collapse handles exactly one
/// padding space per side, like the unquantified `[[:space:]]` in the BRE.
pub fn strip_line(line: &str) -> String {
    // Rule 1: `s/^\([^"]*\)#.*/\1/` — truncate at the first `#` whose
    // preceding text contains no `"`.
    let mut line = line.to_string();
    if let Some(hash) = line.find('#') {
        if !line[..hash].contains('"') {
            line.truncate(hash);
        }
    }
    // Rule 2: `s/^[[:space:]]*//` — strip leading whitespace.
    let line = line.trim_start().to_string();
    // Rule 3: `s/^\([^=[:space:]]*\)[[:space:]]=[[:space:]]/\1=/` —
    // collapse one padding space on each side of the first `=`.
    let mut line = collapse_equals_padding(&line);
    // Rules 4+5 apply to lines without any `"`: remove all whitespace,
    // then strip any remaining `#` comment (redundant after rule 1, kept
    // to mirror the script).
    if !line.contains('"') {
        line = line
            .bytes()
            .filter(|b| !is_space_byte(*b))
            .map(|b| b as char)
            .collect();
        if let Some(hash) = line.find('#') {
            line.truncate(hash);
        }
    }
    // `tr -c -d '\n-~'`: keep only 0x0A..=0x7E (7-bit + newline).
    line.chars()
        .filter(|c| (KEPT_FIRST..=KEPT_LAST).contains(c))
        .collect()
}

/// Collapse exactly one ASCII padding space on each side of the first `=`
/// when the key holds no `=`/whitespace (the BRE `[^=[:space:]]*` prefix).
fn collapse_equals_padding(line: &str) -> String {
    let Some(eq) = line.find('=') else {
        return line.to_string();
    };
    let (left, right) = (&line[..eq], &line[eq + 1..]);
    let (Some(last), Some(first)) = (left.as_bytes().last(), right.as_bytes().first()) else {
        return line.to_string();
    };
    if !is_space_byte(*last) || !is_space_byte(*first) {
        return line.to_string();
    }
    let key = &left[..left.len() - 1];
    if key.contains('=') || key.bytes().any(is_space_byte) {
        return line.to_string();
    }
    format!("{}={}", key, &right[1..])
}

/// Build the framed SysEx payload for rendered patch text.
///
/// Each source line is stripped via [`strip_line`] and re-terminated with
/// `\n` (empty lines contribute a bare newline, as in the script pipeline;
/// `\r` from CRLF sources never reaches the payload because `str::lines`
/// splits on `\n` and strips a trailing `\r`, which rule 4 would delete
/// anyway). The result is inherently 7-bit clean — see
/// [`is_seven_bit_clean`].
pub fn build_sysex(rendered: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(SYSEX_HEADER.len() + rendered.len() + 2);
    out.extend_from_slice(&SYSEX_HEADER);
    for line in rendered.lines() {
        out.extend_from_slice(strip_line(line).as_bytes());
        out.push(b'\n');
    }
    out.push(SYSEX_FOOTER);
    out
}

/// Valentin helper for the send path: every content byte must clear the MSB
/// (MIDI SysEx is 7-bit). Applies to the payload *body* — callers exclude
/// the `F0`/`F7` framing bytes, which legitimately carry the MSB.
/// `build_sysex` bodies always pass.
pub fn is_seven_bit_clean(bytes: &[u8]) -> bool {
    bytes.iter().all(|b| *b < 0x80)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload_text(bytes: &[u8]) -> String {
        let body = &bytes[SYSEX_HEADER.len()..bytes.len() - 1];
        String::from_utf8(body.to_vec()).expect("payload must stay UTF-8")
    }

    #[test]
    fn framing_header_and_footer() {
        let out = build_sysex("[lfo]\nrate = 0.5\n");
        assert_eq!(&out[..5], &SYSEX_HEADER);
        assert_eq!(out[out.len() - 1], SYSEX_FOOTER);
        assert_eq!(out.len(), 5 + "[lfo]\nrate=0.5\n".len() + 1);
    }

    #[test]
    fn empty_input_is_header_plus_footer() {
        assert_eq!(build_sysex(""), vec![0xF0, 0x00, 0x66, 0x66, 0x50, 0xF7]);
    }

    #[test]
    fn output_is_seven_bit_clean() {
        let out = build_sysex("[lfo]\nheader = \"Späß 🎹\"\nrate = 0.5\n");
        let body = &out[SYSEX_HEADER.len()..out.len() - 1];
        assert!(is_seven_bit_clean(body));
    }

    #[test]
    fn validator_rejects_high_bytes() {
        assert!(is_seven_bit_clean(b"abc"));
        assert!(!is_seven_bit_clean(&[0xF0, 0x00]));
        assert!(!is_seven_bit_clean("Spaß".as_bytes()));
    }

    #[test]
    fn strips_padding_and_trailing_comment() {
        assert_eq!(
            payload_text(&build_sysex("input = _FOO # comment\n")),
            "input=_FOO\n"
        );
    }

    #[test]
    fn strips_full_line_banner_comment() {
        assert_eq!(
            payload_text(&build_sysex("# ---- Voice ----\n[lfo]\n")),
            "\n[lfo]\n"
        );
    }

    #[test]
    fn strips_leading_whitespace() {
        assert_eq!(
            payload_text(&build_sysex("   [lfo]\n\tinput = _X\n")),
            "[lfo]\ninput=_X\n"
        );
    }

    #[test]
    fn quoted_string_keeps_hash_and_spaces() {
        assert_eq!(
            payload_text(&build_sysex("header = \"a # b  c\"\n")),
            "header=\"a # b  c\"\n"
        );
    }

    #[test]
    fn comment_strips_when_quote_comes_after_hash() {
        // Rule-1 nuance: only a `"` *before* the `#` protects the comment.
        assert_eq!(
            payload_text(&build_sysex("x = 1 # \"quoted\" later\n")),
            "x=1\n"
        );
    }

    #[test]
    fn non_ascii_dropped_inside_quotes() {
        assert_eq!(
            payload_text(&build_sysex("header = \"Spaß\"\n")),
            "header=\"Spa\"\n"
        );
    }

    #[test]
    fn crlf_source_leaves_no_carriage_return() {
        let out = build_sysex("[lfo]\r\nrate = 0.5\r\n");
        assert!(!out.contains(&b'\r'));
        assert_eq!(payload_text(&out), "[lfo]\nrate=0.5\n");
    }

    #[test]
    fn tab_is_dropped_by_tr_stage() {
        // Tabs vanish even inside quoted strings (tr keeps 0x0A..=0x7E only).
        assert_eq!(
            payload_text(&build_sysex("header = \"a\tb\"\n")),
            "header=\"ab\"\n"
        );
    }

    #[test]
    fn no_patch_dependency_sends_rendered_text_verbatim_shape() {
        // End-to-end shape over representative rendered `.ini` text.
        let rendered = "# DROID patch\n[lfo]\n    rate = 0.5  # hz\n    output = _CLOCK\n[p2b8]\n    button1 = B1.1\n";
        assert_eq!(
            payload_text(&build_sysex(rendered)),
            "\n[lfo]\nrate=0.5\noutput=_CLOCK\n[p2b8]\nbutton1=B1.1\n"
        );
    }
}
