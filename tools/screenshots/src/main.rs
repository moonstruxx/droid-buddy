//! Deterministic README screenshot harness (change `readme-docs`, task 1.1).
//!
//! Renders `docs/assets/*.png` through the app's real egui paint path — the
//! same `paint_panes` / `paint_overlays` dispatch the window loop runs —
//! inside a headless wgpu renderer (`egui_kittest`, `PREDICTABLE` options).
//! Every shot renders twice from fresh state; the harness fails when the two
//! runs differ (nondeterminism) or when a PNG on disk no longer matches a
//! fresh render (stale screenshot). `--bless` rewrites the PNGs.
//!
//! Tools-only crate by task contract: it must not modify `src/`, yet it must
//! paint through `pub(crate)` draw routines. It does both by compiling the
//! app sources in place — each module is shadowed with `#[path]`, and `gui`
//! is textually included so the shot driver lives inside the paint module and
//! sees its private dispatch. No source is copied, so renders always track the
//! current paint path. `CARGO_MANIFEST_DIR`-relative data (`rack_geometry.json`,
//! `controller_geometry.json`) resolves via symlinks next to this manifest.
//!
//! Determinism notes:
//! - Fixed viewport (1280x800, 1.0 px/pt), classic theme, isolated empty XDG
//!   config dir (no user labels/plugins leak in), three warmup frames so the
//!   graph camera seeds from published pane geometry before the capture.
//! - The graph settle animation is time-based, so it is cancelled after setup:
//!   captures show solved positions, never a mid-flight interpolation.
//! - kittest disables cursor blink and scroll animation; font rasterization is
//!   CPU-side and the wgpu renderer runs with predictable options.

// Module shadows: same set and order as `src/lib.rs` (minus `regression`,
// which is `cfg(test)`-only). `crate::X` paths inside the sources resolve to
// these shadows, so the harness compiles the current app code unmodified.
#[path = "../../../src/app.rs"]
pub mod app;
#[path = "../../../src/config.rs"]
pub mod config;
#[path = "../../../src/diff.rs"]
pub mod diff;
#[path = "../../../src/events.rs"]
pub mod events;
#[path = "../../../src/expression.rs"]
pub mod expression;
#[path = "../../../src/favorites.rs"]
pub mod favorites;
#[path = "../../../src/geometry.rs"]
pub mod geometry;
#[path = "../../../src/graph.rs"]
pub mod graph;
#[path = "../../../src/graph_anim.rs"]
pub mod graph_anim;
#[path = "../../../src/graph_render.rs"]
pub mod graph_render;
#[path = "../../../src/handler.rs"]
pub mod handler;
#[path = "../../../src/help.rs"]
pub mod help;
#[path = "../../../src/latency.rs"]
pub mod latency;
#[path = "../../../src/layout.rs"]
pub mod layout;
#[path = "../../../src/optimize.rs"]
pub mod optimize;
#[path = "../../../src/panes.rs"]
pub mod panes;
#[path = "../../../src/patch.rs"]
pub mod patch;
#[path = "../../../src/performance.rs"]
pub mod performance;
#[path = "../../../src/physical.rs"]
pub mod physical;
#[path = "../../../src/plugin.rs"]
pub mod plugin;
#[path = "../../../src/rendermetrics.rs"]
pub mod rendermetrics;
#[path = "../../../src/schema.rs"]
pub mod schema;
#[path = "../../../src/sysex.rs"]
pub mod sysex;
#[path = "../../../src/theme.rs"]
pub mod theme;
#[path = "../../../src/validation.rs"]
pub mod validation;

/// The paint module, textually included: `mod graph;` et al. inside resolve to
/// `src/gui/*.rs` (relative to the included file), and the shot driver below
/// shares the module so it can call the private `paint_panes` dispatch.
pub mod gui {
    include!(concat!(env!("OUT_DIR"), "/gui_mod.rs"));

    /// Paint one full-window frame, mirroring the window loop's
    /// draw→read→dispatch body: arrangement panes (or the bare graph canvas
    /// with nothing open) plus overlays. No selection, no hover, no input.
    pub fn paint_frame(app: &mut App, ui: &mut egui::Ui) {
        let window_rect = ui.max_rect();
        let band = to_cell_rect(window_rect);
        // Mirrors the `scene_for` headless helper: the scene is framed by the
        // graph pane when one is open, else by the whole window.
        let scene = app.graph.as_ref().and_then(|_| {
            build_scene_spec(
                app,
                crate::theme::active(),
                graph_pane_rect(app, window_rect).unwrap_or(window_rect),
            )
        });
        if band_has_views(app, band) {
            paint_panes(app, ui, scene.as_ref(), &[]);
        } else {
            app.pane_rects.clear();
            app.component_rects.clear();
            graph::paint_scene(ui, window_rect.size(), scene.as_ref(), &[]);
        }
        paint_overlays(app, ui);
    }
}

use std::io::Cursor;
use std::path::{Path, PathBuf};

use app::{App, ViewType};
use panes::PaneId;

const VIEWPORT: (f32, f32) = (1280.0, 800.0);
const WARMUP_FRAMES: u64 = 3;

/// One render: fixture + arrangement setup. `save` shots land in `docs/assets`
/// (the `docs/assets/README.md` contract); the large-patch renders only prove
/// scale stability and are hash-compared, never saved.
struct Shot {
    /// Output file name under the out dir (empty = stability-only, not saved).
    file: &'static str,
    fixture: &'static str,
    setup: fn(&mut App),
}

fn setup_startup(_app: &mut App) {}

fn setup_graph(app: &mut App) {
    // Two-big-pane arrangement (module UI left, graph right): close the
    // startup source viewer first — while a small-class view is open the
    // right half splits and a big-right graph would stay invisible. Focus
    // moves through the public carousel so the legacy focus mirror stays in
    // sync (a direct `layout.focus` write would be clobbered by the next
    // reconcile).
    app.cycle_focus(true);
    app.close_focused_view();
    app.layout.focus = PaneId::BigRight;
    app.open_view(ViewType::Graph);
    app.cancel_graph_settle();
}

fn setup_viewer(app: &mut App) {
    // Source-viewer close-up: focus its small pane through the public
    // carousel (startup focus is BigLeft, SmallTop is next in tree order),
    // then maximize it to the full band.
    app.cycle_focus(true);
    app.maximize_toggle();
}

fn setup_optimizer(app: &mut App) {
    app.open_view(ViewType::Optimizer);
}

fn shots() -> Vec<Shot> {
    vec![
        Shot {
            file: "module-ui.png",
            fixture: "fixtures/own_buttons.ini",
            setup: setup_startup,
        },
        Shot {
            file: "graph.png",
            fixture: "fixtures/own_buttons.ini",
            setup: setup_graph,
        },
        Shot {
            file: "source-viewer.png",
            fixture: "fixtures/source_navigation.ini",
            setup: setup_viewer,
        },
        Shot {
            file: "optimizer.png",
            fixture: "fixtures/optimizer_latency.ini",
            setup: setup_optimizer,
        },
        // The scale anchor (perf-gate `SCALE_ANCHOR`, 509 sections): renders
        // must be stable here too, not just on the small specs.
        Shot {
            file: "",
            fixture: "fixtures/own_scale.ini",
            setup: setup_startup,
        },
        Shot {
            file: "",
            fixture: "fixtures/own_scale.ini",
            setup: setup_graph,
        },
    ]
}

fn load_app(root: &Path, fixture: &str, setup: fn(&mut App)) -> App {
    let mut app = App::new();
    let patch =
        patch::Patch::from_ini_file(&root.join(fixture)).expect("screenshot fixture must parse");
    assert!(app.load_patch(patch), "screenshot fixture must load");
    setup(&mut app);
    app.cancel_graph_settle();
    app
}

/// Render one shot to PNG bytes: fresh app, fixed viewport, warmup frames so
/// pane-published geometry (graph canvas, camera fit) settles, then capture.
fn render_png(root: &Path, shot: &Shot) -> Vec<u8> {
    let app = load_app(root, shot.fixture, shot.setup);
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(VIEWPORT.0, VIEWPORT.1))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_ui_state(
            |ui: &mut egui::Ui, app: &mut App| {
                gui::paint_frame(app, ui);
            },
            app,
        );
    for _ in 0..WARMUP_FRAMES {
        harness.step();
    }
    let image = harness.render().expect("headless wgpu render must succeed");
    assert_eq!(
        (image.width(), image.height()),
        (VIEWPORT.0 as u32, VIEWPORT.1 as u32),
        "capture must match the fixed viewport"
    );
    // Guard against blank captures: a real UI is never one flat color.
    let first = image.pixels().next().expect("capture has pixels");
    assert!(
        image.pixels().any(|p| p != first),
        "capture must not be a single flat color"
    );
    let dyn_image = image::DynamicImage::ImageRgba8(image);
    let mut png = Vec::new();
    dyn_image
        .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("PNG encode must succeed");
    png
}

/// Stable display hash (FNV-1a/64) for logs; byte equality is the real check.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn usage() -> ! {
    eprintln!("usage: screenshots [--root DIR] [--out DIR] [--bless] [--only a,b,c]");
    eprintln!("  shots: module-ui, graph, source-viewer, optimizer, large-startup, large-graph");
    std::process::exit(2);
}

fn main() {
    let mut root = PathBuf::from(".");
    let mut out: Option<PathBuf> = None;
    let mut bless = false;
    let mut only: Option<Vec<String>> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => root = PathBuf::from(args.next().unwrap_or_else(|| usage())),
            "--out" => out = Some(PathBuf::from(args.next().unwrap_or_else(|| usage()))),
            "--bless" => bless = true,
            "--only" => {
                only = Some(
                    args.next()
                        .unwrap_or_else(|| usage())
                        .split(',')
                        .map(str::to_string)
                        .collect(),
                );
            }
            _ => usage(),
        }
    }
    let out = out.unwrap_or_else(|| root.join("docs/assets"));

    // Isolate user config: an empty XDG dir means default theme/labels/plugins
    // on every machine, so no local `labels.toml` or `config.toml` can shift
    // a single pixel between runs.
    let xdg = std::env::temp_dir().join(format!("droid-shots-{}.xdg", std::process::id()));
    std::fs::create_dir_all(&xdg).expect("scratch XDG dir");
    // SAFETY: single-threaded startup, before any thread reads the env.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &xdg);
        std::env::set_var("HOME", &xdg);
    }
    theme::init(theme::Theme::classic());

    let names = [
        "module-ui",
        "graph",
        "source-viewer",
        "optimizer",
        "large-startup",
        "large-graph",
    ];
    let mut failures = 0;
    for (shot, name) in shots().iter().zip(names) {
        if let Some(only) = &only {
            if !only.iter().any(|o| o == name) {
                continue;
            }
        }
        let first = render_png(&root, shot);
        let second = render_png(&root, shot);
        if first != second {
            eprintln!(
                "FAIL {name}: two fresh renders differ ({} vs {} bytes) — nondeterministic paint",
                first.len(),
                second.len()
            );
            failures += 1;
            continue;
        }
        if shot.file.is_empty() {
            println!(
                "ok   {name}: stable across runs ({} bytes, fnv {:016x})",
                first.len(),
                fnv1a(&first)
            );
            continue;
        }
        let path = out.join(shot.file);
        if bless {
            std::fs::create_dir_all(&out).expect("out dir");
            std::fs::write(&path, &first).expect("write PNG");
            println!(
                "blessed {} ({} bytes, fnv {:016x})",
                path.display(),
                first.len(),
                fnv1a(&first)
            );
            continue;
        }
        match std::fs::read(&path) {
            Ok(on_disk) if on_disk == first => {
                println!(
                    "ok   {name}: {} fresh ({} bytes)",
                    path.display(),
                    first.len()
                );
            }
            Ok(_) => {
                eprintln!(
                    "FAIL {name}: {} is stale — re-run with --bless",
                    path.display()
                );
                failures += 1;
            }
            Err(_) => {
                eprintln!(
                    "FAIL {name}: {} missing — re-run with --bless",
                    path.display()
                );
                failures += 1;
            }
        }
    }
    let _ = std::fs::remove_dir_all(&xdg);
    if failures > 0 {
        eprintln!("{failures} screenshot check(s) failed");
        std::process::exit(1);
    }
    println!("all screenshot checks passed");
}
