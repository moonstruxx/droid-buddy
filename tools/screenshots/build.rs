//! Build script: stages `src/gui/mod.rs` for textual inclusion.
//!
//! The harness compiles the paint module in place (`include!` inside its own
//! `pub mod gui`, so the shot driver shares the module and can call the
//! private `paint_panes` dispatch). `include!` rejects inner doc comments
//! (`//!`, E0753), so the staged copy converts them to outer docs — the only
//! transformation applied; everything else is byte-identical.

fn main() {
    let shadowed = [
        "app",
        "config",
        "diff",
        "events",
        "expression",
        "favorites",
        "geometry",
        "graph",
        "graph_anim",
        "graph_render",
        "handler",
        "help",
        "latency",
        "layout",
        "optimize",
        "panes",
        "patch",
        "performance",
        "physical",
        "plugin",
        "rendermetrics",
        "schema",
        "sysex",
        "theme",
        "validation",
    ];
    for module in shadowed {
        println!("cargo::rerun-if-changed=../../src/{module}.rs");
    }
    for surface in ["graph", "overlays", "physical", "picker", "viewer"] {
        println!("cargo::rerun-if-changed=../../src/gui/{surface}.rs");
    }
    println!("cargo::rerun-if-changed=../../src/gui/mod.rs");

    let root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("MANIFEST"))
        .join("../..")
        .canonicalize()
        .expect("repo root");
    let src = std::fs::read_to_string(root.join("src/gui/mod.rs")).expect("read gui/mod.rs");
    // The staged copy lives in OUT_DIR, so the surface `mod x;` declarations
    // must carry absolute paths back to the real sources.
    let surfaces = ["graph", "overlays", "physical", "picker", "viewer"];
    let staged: Vec<String> = src
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if let Some(name) = trimmed
                .strip_prefix("mod ")
                .and_then(|s| s.strip_suffix(';'))
            {
                if surfaces.contains(&name) {
                    let path = root.join(format!("src/gui/{name}.rs"));
                    return format!("#[path = \"{}\"] mod {name};", path.display());
                }
            }
            match line.strip_prefix("//!") {
                Some(rest) => format!("///{rest}"),
                None => line.to_string(),
            }
        })
        .collect();
    let out =
        std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("gui_mod.rs");
    std::fs::write(out, staged.join("\n")).expect("write staged gui/mod.rs");
}
