//! Materialize `evidence/gallery/index.html` via the same TestBackend
//! rendering as the insta tests. Entrypoint for `cargo run --bin snapshot-gallery`
//! and the manual side of `cargo test -- --generate-gallery`.

use std::process;

fn main() {
    match droid_tui::gallery::generate_gallery() {
        Ok(path) => {
            println!("gallery written to {}", path.display());
        }
        Err(err) => {
            eprintln!("failed to generate gallery: {err}");
            process::exit(1);
        }
    }
}
