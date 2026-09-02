//! Kitty graphics protocol transport (`\x1b_G` escapes, no `ratatui-image`).
//!
//! Implements the chunked base64+zlib transmit (`o=z`, ≤4096-byte chunks),
//! cell placement (`a=p`), delete (`a=d`), cursor positioning, and capability
//! detection (design.md decisions 1, 4, 6, 8, 10). `q=2` (suppress failures)
//! rides on every command so ACK noise never reaches the single-threaded
//! crossterm stdin.
//!
//! Gating: the pure payload builders below are always compiled so they
//! unit-test in the default suite; only the terminal-write `emit` and the
//! `detect` handshake are gated behind `#[cfg(feature = "kitty-gfx")]` — the
//! module *is* the transport, and the escape-emitting compile requires the
//! feature. `TestBackend`/snapshot rendering never calls into this module.

use std::io::{self, Write};

/// Maximum base64 payload bytes per escape chunk (kitty protocol limit).
pub const MAX_CHUNK_SIZE: usize = 4096;

const ESC_G: &str = "\x1b_G";
const ESC_ST: &str = "\x1b\\";

// ---------------------------------------------------------------------------
// Pure payload builders — always compiled, unit-tested.
// ---------------------------------------------------------------------------

/// zlib (RFC-1950) compress + base64-encode an RGBA payload for `o=z` transmit.
pub fn encode_payload(rgba: &[u8]) -> io::Result<String> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(rgba)?;
    Ok(STANDARD.encode(encoder.finish()?))
}

/// Split encoded payload text into ≤ `max_chunk`-byte slices (never empty).
pub fn chunk_encoded(encoded: &str, max_chunk: usize) -> Vec<&str> {
    let step = max_chunk.max(1);
    let mut chunks = Vec::new();
    let mut rest = encoded;
    while !rest.is_empty() {
        let (head, tail) = rest.split_at(rest.len().min(step));
        chunks.push(head);
        rest = tail;
    }
    if chunks.is_empty() {
        chunks.push("");
    }
    chunks
}

fn chunk_flag(more: bool) -> u8 {
    // Kitty graphics protocol: `m=1` for all but the last chunk, `m=0` for the
    // last. The terminal buffers the image until the final chunk arrives, so an
    // inverted flag leaves a single-chunk payload (or the final chunk) unflushed
    // and the image is never displayed.
    if more {
        1
    } else {
        0
    }
}

/// First transmit chunk escape: full control data, `s`/`v`/`o=z` ride here only.
pub fn transmit_first_chunk(id: u32, width: u32, height: u32, chunk: &str, more: bool) -> String {
    let m = chunk_flag(more);
    format!("{ESC_G}a=t,i={id},f=32,s={width},v={height},o=z,m={m},q=2;{chunk}{ESC_ST}")
}

/// Continuation chunk escape: carries only `m` (and `q`) per design.md.
pub fn transmit_cont_chunk(chunk: &str, more: bool) -> String {
    let m = chunk_flag(more);
    format!("{ESC_G}m={m},q=2;{chunk}{ESC_ST}")
}

/// Every transmit escape for an RGBA payload, in wire order.
pub fn transmit_escapes(id: u32, width: u32, height: u32, rgba: &[u8]) -> io::Result<Vec<String>> {
    if rgba.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cannot transmit an empty RGBA payload",
        ));
    }
    let encoded = encode_payload(rgba)?;
    let chunks = chunk_encoded(&encoded, MAX_CHUNK_SIZE);
    let last = chunks.len() - 1;
    Ok(chunks
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let more = i != last;
            if i == 0 {
                transmit_first_chunk(id, width, height, c, more)
            } else {
                transmit_cont_chunk(c, more)
            }
        })
        .collect())
}

/// Place escape: scale image `id` into the cell at (col, row), `z=-1` under
/// text, `C=1` leaves the cursor put. Re-transmitting the same `i=` replaces
/// the prior placement, so pan/zoom never exhausts image ids.
pub fn place_escape(id: u32, col: u16, row: u16) -> String {
    format!("{ESC_G}a=p,i={id},c={col},r={row},z=-1,C=1,q=2{ESC_ST}")
}

/// Delete escape: remove all placed images (cleanup on exit/fallback).
pub fn delete_escape() -> String {
    format!("{ESC_G}a=d,q=2{ESC_ST}")
}

/// Cursor positioning escape, 1-based `row;col` — emitted before each image.
pub fn cursor_escape(row: u16, col: u16) -> String {
    format!("\x1b[{row};{col}H")
}

/// Kitty graphics query escape for the detection handshake (no `q` key: the
/// response must come back, so this one command is *not* quieted).
pub fn detect_query_escape() -> String {
    format!("{ESC_G}i=31,s=1,v=1,a=q,t=d,f=24;AAAA{ESC_ST}")
}

/// DA1 (primary device attributes) query escape, sent after the graphics query.
pub fn da1_escape() -> String {
    "\x1b[c".to_string()
}

/// Fast detection path: kitty exports `KITTY_WINDOW_ID` to child processes
/// (kitty and compatibles such as wezterm with kitty graphics enabled).
pub fn env_signal() -> bool {
    std::env::var("KITTY_WINDOW_ID").is_ok()
}

/// Does a captured handshake response prove kitty graphics support? A kitty
/// terminal answers the `a=q,t=d,f=24` query with `\x1b_Gi=31;OK\x1b\\` and its
/// DA1 response carries kitty's device id 62 (`\x1b[?62...c`).
pub fn handshake_supported(response: &[u8]) -> bool {
    let text = String::from_utf8_lossy(response);
    text.contains("\x1b_Gi=31;OK") || text.contains("\x1b[?62")
}

// ---------------------------------------------------------------------------
// Feature-gated facade: terminal writes + capability detection.
// ---------------------------------------------------------------------------

#[cfg(feature = "kitty-gfx")]
mod emit {
    use super::*;
    use std::io::Read;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Cache: probed once, then stable for the process lifetime. `AtomicBool`
    /// + a probed flag (rather than a bare `OnceLock`) so tests can reset it.
    static SUPPORTED: AtomicBool = AtomicBool::new(false);
    static PROBED: AtomicBool = AtomicBool::new(false);

    /// Cached kitty-graphics capability: `KITTY_WINDOW_ID` fast path, else the
    /// `a=q`/DA1 handshake. Non-kitty terminals fall back to box-drawing.
    pub fn supported() -> bool {
        if !PROBED.swap(true, Ordering::Relaxed) {
            SUPPORTED.store(probe(), Ordering::Relaxed);
        }
        SUPPORTED.load(Ordering::Relaxed)
    }

    fn probe() -> bool {
        if env_signal() {
            return true;
        }
        let mut out = io::stdout();
        let _ = out.write_all(detect_query_escape().as_bytes());
        let _ = out.write_all(da1_escape().as_bytes());
        let _ = out.flush();
        read_handshake_response().is_some_and(|r| handshake_supported(&r))
    }

    /// Bounded stdin read so a non-kitty terminal never hangs startup.
    fn read_handshake_response() -> Option<Vec<u8>> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            let n = io::stdin().read(&mut buf).unwrap_or(0);
            if n > 0 {
                let _ = tx.send(buf[..n].to_vec());
            }
        });
        rx.recv_timeout(std::time::Duration::from_millis(250)).ok()
    }

    /// Transmit an RGBA image as chunked escapes (same `i=` replaces in place).
    pub fn transmit(id: u32, width: u32, height: u32, rgba: &[u8]) -> io::Result<()> {
        for esc in super::transmit_escapes(id, width, height, rgba)? {
            write_escape(&esc)?;
        }
        Ok(())
    }

    /// Place image `id` into the cell at (col, row), `z=-1`, cursor unmoved.
    pub fn place(id: u32, col: u16, row: u16) -> io::Result<()> {
        write_escape(&super::place_escape(id, col, row))
    }

    /// Remove all placed images (`z=-1` cleanup on exit/fallback).
    pub fn delete() -> io::Result<()> {
        write_escape(&super::delete_escape())
    }

    /// Move the cursor to a 1-based cell (the graph area's top-left) first.
    pub fn cursor(row: u16, col: u16) -> io::Result<()> {
        write_escape(&super::cursor_escape(row, col))
    }

    /// Cursor → transmit → place: the per-frame image update for one area.
    pub fn frame(
        id: u32,
        width: u32,
        height: u32,
        rgba: &[u8],
        col: u16,
        row: u16,
    ) -> io::Result<()> {
        cursor(row, col)?;
        transmit(id, width, height, rgba)?;
        place(id, col, row)
    }

    fn write_escape(esc: &str) -> io::Result<()> {
        let mut out = io::stdout().lock();
        out.write_all(esc.as_bytes())?;
        out.flush()
    }

    #[cfg(test)]
    pub fn reset_for_tests() {
        PROBED.store(false, Ordering::Relaxed);
        SUPPORTED.store(false, Ordering::Relaxed);
    }

    /// Test-only override so render-dispatch tests can force either branch of
    /// the `supported()` gate without a terminal handshake.
    #[cfg(test)]
    pub fn set_supported_for_tests(value: bool) {
        PROBED.store(true, Ordering::Relaxed);
        SUPPORTED.store(value, Ordering::Relaxed);
    }
}

#[cfg(feature = "kitty-gfx")]
pub use emit::{cursor, delete, frame, place, supported, transmit};
#[cfg(all(test, feature = "kitty-gfx"))]
pub use emit::{reset_for_tests, set_supported_for_tests};

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic pseudo-random RGBA payload (LCG high byte): incompressible
    /// enough that zlib cannot shrink it below the 4096-byte chunk threshold.
    fn lcg_payload(len: usize) -> Vec<u8> {
        let mut state: u32 = 0x1234_5678;
        (0..len)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 24) as u8
            })
            .collect()
    }

    #[test]
    fn transmit_escape_matches_known_rgba_payload() {
        let rgba = [255, 0, 0, 255, 0, 255, 0, 255];
        let escapes = transmit_escapes(1, 2, 1, &rgba).unwrap();
        assert_eq!(escapes.len(), 1);
        // Byte-exact full escape string for the known 2×1 RGBA payload
        // (base64 of the flate2 zlib stream of those 8 bytes, captured from
        // the real encoder — see commit note; do not hand-recompute).
        assert_eq!(
            escapes[0],
            "\u{1b}_Ga=t,i=1,f=32,s=2,v=1,o=z,m=0,q=2;eJz7z8DwHwQBEPcD/Q==\u{1b}\\"
        );
    }

    #[test]
    fn large_payload_chunks_with_correct_boundaries() {
        let rgba = lcg_payload(20_000);
        let escapes = transmit_escapes(1, 20_000, 1, &rgba).unwrap();
        assert!(escapes.len() > 1, "expected multiple chunks");

        // First chunk: full control data, m=1 (more chunks follow).
        assert!(
            escapes[0].starts_with("\x1b_Ga=t,i=1,f=32,s=20000,v=1,o=z,m=1,q=2;"),
            "first chunk control data wrong: {:?}",
            escapes[0]
        );
        // Last chunk: m=0 (last), continuation form.
        assert!(
            escapes.last().unwrap().starts_with("\x1b_Gm=0,q=2;"),
            "last chunk must be m=0 continuation: {:?}",
            escapes.last().unwrap()
        );
        // Intermediate chunks: only m (and q), never the transmit control data.
        for esc in &escapes[1..escapes.len() - 1] {
            assert!(
                esc.starts_with("\x1b_Gm=1,q=2;"),
                "intermediate chunk must be m=1 continuation: {esc:?}"
            );
            assert!(
                !esc.contains("a=t"),
                "continuation must not repeat control data: {esc:?}"
            );
            assert!(!esc.contains("i=") && !esc.contains("f=") && !esc.contains("o=z"));
        }

        // Every chunk's payload (text after `;`) stays ≤ MAX_CHUNK_SIZE.
        let mut reassembled = String::new();
        for esc in &escapes {
            let payload = esc
                .strip_prefix("\x1b_G")
                .and_then(|s| s.split_once(';'))
                .map(|(_, p)| p.strip_suffix("\x1b\\").unwrap_or(""))
                .unwrap_or("");
            assert!(
                payload.len() <= MAX_CHUNK_SIZE,
                "chunk payload too large: {}",
                payload.len()
            );
            reassembled.push_str(payload);
        }
        // Reassembly round-trips the encoded payload exactly.
        assert_eq!(reassembled, encode_payload(&rgba).unwrap());
    }

    #[test]
    fn suppress_flag_on_every_command() {
        let rgba = lcg_payload(10_000);
        let escapes = transmit_escapes(7, 10_000, 1, &rgba).unwrap();
        let mut all = escapes;
        all.push(place_escape(7, 12, 34));
        all.push(delete_escape());
        for esc in &all {
            assert!(esc.contains("q=2"), "every command must carry q=2: {esc:?}");
        }
    }

    #[test]
    fn place_delete_cursor_exact_escapes() {
        assert_eq!(
            place_escape(7, 12, 34),
            "\x1b_Ga=p,i=7,c=12,r=34,z=-1,C=1,q=2\x1b\\"
        );
        assert_eq!(delete_escape(), "\x1b_Ga=d,q=2\x1b\\");
        assert_eq!(cursor_escape(3, 5), "\x1b[3;5H");
    }

    #[test]
    fn detection_query_escapes_are_exact() {
        assert_eq!(
            detect_query_escape(),
            "\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\"
        );
        assert_eq!(da1_escape(), "\x1b[c");
    }

    #[test]
    fn handshake_response_detects_kitty() {
        // kitty answers the a=q query with OK and DA1 with device id 62.
        assert!(handshake_supported(b"\x1b_Gi=31;OK\x1b\\"));
        assert!(handshake_supported(b"\x1b[?62;1;2c"));
        assert!(handshake_supported(b"\x1b_Gi=31;OK\x1b\\\x1b[?62;1;2c"));
        // xterm's DA1 (device 1) must not match.
        assert!(!handshake_supported(b"\x1b[?1;2c"));
        assert!(!handshake_supported(b""));
    }

    #[test]
    fn chunk_encoded_splits_within_limit() {
        let text = "x".repeat(10_000);
        let chunks = chunk_encoded(&text, MAX_CHUNK_SIZE);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), MAX_CHUNK_SIZE);
        assert_eq!(chunks[1].len(), MAX_CHUNK_SIZE);
        assert_eq!(chunks[2].len(), 10_000 - 2 * MAX_CHUNK_SIZE);
        assert_eq!(chunks.concat(), text);
        // Degenerate inputs still produce one chunk.
        assert_eq!(chunk_encoded("", MAX_CHUNK_SIZE), vec![""]);
        assert_eq!(chunk_encoded("ab", 0), vec!["a", "b"]);
    }

    #[test]
    fn empty_payload_is_rejected() {
        // Task 3.2: an empty buffer has no image to transmit; the wire would
        // otherwise receive a zero-data escape stream, so the caller must get
        // an error to fall back from.
        assert!(transmit_escapes(1, 0, 0, &[]).is_err());
        assert!(transmit_escapes(1, 10, 10, &[]).is_err());
    }
}

#[cfg(all(test, feature = "kitty-gfx"))]
mod feature_tests {
    use super::*;

    #[test]
    fn feature_gated_emit_keeps_pure_builder_escape_shapes() {
        // Task 3.2: with the kitty-gfx feature on, the terminal-write `emit`
        // facade compiles; its string builders must stay byte-identical to the
        // always-compiled path so a feature build ships the same wire bytes.
        // Only string assembly is exercised — the handshake probe
        // (`emit::supported`) needs a real TTY and is never run from tests.
        emit::reset_for_tests();
        assert_eq!(
            place_escape(7, 12, 34),
            "\x1b_Ga=p,i=7,c=12,r=34,z=-1,C=1,q=2\x1b\\"
        );
        assert_eq!(delete_escape(), "\x1b_Ga=d,q=2\x1b\\");
        assert_eq!(cursor_escape(3, 5), "\x1b[3;5H");
        let escapes = transmit_escapes(1, 2, 1, &[255, 0, 0, 255, 0, 255, 0, 255]).unwrap();
        assert_eq!(
            escapes[0],
            "\u{1b}_Ga=t,i=1,f=32,s=2,v=1,o=z,m=0,q=2;eJz7z8DwHwQBEPcD/Q==\u{1b}\\"
        );
    }
}
