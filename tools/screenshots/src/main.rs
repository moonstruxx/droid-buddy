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

/// The annotation-layer shot (change `module-ui-labels`, task 3.4): the startup
/// arrangement at true 1:1, the size `fixtures/module_ui_labels.ini`'s preamble
/// labels were chosen for — at 1:1 a 9 mm B32 button cell budgets five 10 pt
/// characters, so "Trigger input" and "Clock reset" clip while "Go" fits, and
/// the default `auto` presentation annotates exactly the two clipped labels.
///
/// The startup fit would scale this ~91 mm rack past 1:1 (the harness pane is
/// taller than one true-size rack), which both hides the clipping and leaves no
/// side band for the callouts, so the shot pins the zoom. The camera is centred
/// for the pinned zoom without hard-coding a layout number: the startup pane is
/// published through the same `refresh_hit_geometry` call the first painted
/// frame makes, one fit gives the rack's real extent (the fit offset is
/// `(rack·zoom − pane)/2` per axis), and the pinned offset re-centres that same
/// extent. The manual-navigation latch then keeps the pane-resize trigger from
/// refitting over the pinned camera. Field writes before paint only —
/// deterministic, no input.
fn setup_labels_auto(app: &mut App) {
    setup_startup(app);
    app.refresh_hit_geometry(app::Rect::new(0, 0, VIEWPORT.0 as u16, VIEWPORT.1 as u16));
    let Some((_, pane)) = app
        .pane_hit_rects
        .iter()
        .find(|(id, _)| *id == PaneId::BigLeft)
    else {
        return;
    };
    let (pane_w, pane_h) = (pane.width as f32, pane.height as f32);
    if !app.fit_physical_view(pane_w, pane_h) {
        return;
    }
    let fit = app.physical_zoom as f64;
    let (off_x, off_y) = (app.physical_offset.0 as f64, app.physical_offset.1 as f64);
    let rack_w = (pane_w as f64 + 2.0 * off_x) / fit;
    let rack_h = (pane_h as f64 + 2.0 * off_y) / fit;
    app.physical_zoom = 1.0;
    app.scale_factor = 1.0;
    app.physical_offset = (
        ((rack_w - pane_w as f64) / 2.0) as f32,
        ((rack_h - pane_h as f64) / 2.0) as f32,
    );
    // 1:1 is the shot's subject, not the pane's fit, so the title carries the
    // plain zoom; the latch keeps the first frame's fit trigger inert.
    app.physical_fit_zoom = None;
    app.physical_fit_pending = false;
    app.physical_nav_latched = true;
}

/// The startup arrangement, zoomed so the module UI's rack fills its pane.
/// The six-controller faceplate fixture draws a ~207 mm rack — wider than any
/// other shot's — so at the default 1:1 zoom its faceplates are only ~38 px
/// wide and the element stacks are hard to judge. The 2x seed below predates
/// the true-1:1 rebaseline (then 207 mm at 2x spanned 620 x 766 px); since
/// `module-ui-labels` the pane fits itself on its first frame, so the fit
/// supersedes this seed and the shot opens at the pane's fit zoom instead.
fn setup_faceplate_stack(app: &mut App) {
    setup_startup(app);
    app.physical_zoom = 2.0;
}

/// The shift-tint arrangement (change `shift-colour-module-ui`, task 3.2):
/// the 2x faceplate-stack zoom plus an active Group2, so member faces tint
/// in the group token. A plain field write before paint — deterministic, no
/// input — and the double-render check enforces it.
fn setup_shift_colour(app: &mut App) {
    setup_faceplate_stack(app);
    app.active_shift = Some(patch::ShiftGroup::Group2);
}

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

/// The sections-mode arrangement (change `sections-black-hole-solver`,
/// task 7.1): the same two-big-pane graph layout as [`setup_graph`], but with
/// the third arrangement mode selected *before* the graph is placed, so the
/// first solve runs through `layout::solve_sections` (patch-order lanes,
/// width-aware, wrapped into rows) rather than column or force mode. A plain
/// field write before paint — no input, deterministic — and the harness's
/// double-render check enforces it. The camera is left unset so the paint
/// path's first-frame fit frames the whole solved layout.
fn setup_sections(app: &mut App) {
    app.layout_mode = config::LayoutMode::Sections;
    setup_graph(app);
}

/// The force-arrangement shot (change `force-arrangement-nonoverlap-and-sliders`,
/// task 4.3): the same two-big-pane graph layout as [`setup_graph`], but with
/// the force arrangement selected *before* the graph is placed, so the first
/// solve runs through `layout::solve` — size-aware repulsion plus the
/// converging separation post-condition — rather than column or sections mode.
/// The arrangement-aware `S` panel is switched on as well, because the force
/// arrangement is where it gained its own rows (Repulsion / Cohesion / Cable).
/// Both are plain field writes before paint — no input, deterministic — and the
/// harness's double-render check enforces it. The camera is left unset so the
/// paint path's first-frame fit frames the whole solved layout.
fn setup_force_sliders(app: &mut App) {
    app.layout_mode = config::LayoutMode::Force;
    setup_graph(app);
    app.showing_sliders = true;
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
        // Label-annotation check (change `module-ui-labels`, task 3.4): the
        // fixture's preamble labels are sized for the B32's 9 mm button cells
        // at true 1:1, so "Trigger input" and "Clock reset" overflow their
        // cells while "Go" fits. Under the default `auto` presentation the
        // two clipped labels become side-bound callouts with leader lines and
        // "Go" is never annotated, in the same frame that shows the rack at
        // true size (fit-on-open, centred). No field write beyond the startup
        // arrangement: the annotation layer is the shipped default.
        Shot {
            file: "module-ui-labels.png",
            fixture: "fixtures/module_ui_labels.ini",
            setup: setup_labels_auto,
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
        // Faceplate fidelity check (change `controller-geometry-fidelity`,
        // task 3.2): one patch declaring all six corrected controllers, so the
        // P2B8/P4B2/P10 pot stacks, the S10 rotary stack, the E4 encoder stack
        // with its 32-LED rings, and the DB8E encoder below its buttons all
        // render in one startup frame. The startup band's half-width left pane
        // holds the whole ~206 mm rack at the default zoom.
        Shot {
            file: "faceplate-rotary-stack.png",
            fixture: "fixtures/faceplate_rotary_stack.ini",
            setup: setup_faceplate_stack,
        },
        // Button-face + LED-fold check (change `button-led-composition`,
        // task 4.2): P2B8 + B32 round faces and the M4 touch-plate rectangle
        // with their folded LEDs, plus one LED-less pot proving the
        // shape-alone path, in one startup frame.
        Shot {
            file: "button-led-composition.png",
            fixture: "fixtures/button_led_composition.ini",
            setup: setup_faceplate_stack,
        },
        // Static LED-colour check (change `led-register-colour`, task 3.2):
        // a lit P2B8 button face and a lit M4 touch plate in their resolved
        // colours beside the dynamic-cable and sourceless white fallbacks,
        // in one startup frame. Two controllers only, so the 2x stack zoom
        // fits with margin (tighter zooms clipped the M4 plate last time).
        Shot {
            file: "led-register-colour.png",
            fixture: "fixtures/led_register_colour.ini",
            setup: setup_faceplate_stack,
        },
        // Unreferenced-elements check (change `unreferenced-elements`,
        // task 3.2): a partial B32 (5 of 32 buttons) beside a bare E4, so
        // the 27 dimmed unlabelled buttons and the 124 dimmed ring segments
        // read in one startup frame. Two controllers only, so the 2x stack
        // zoom fits with margin.
        Shot {
            file: "unreferenced-elements.png",
            fixture: "fixtures/unreferenced_elements.ini",
            setup: setup_faceplate_stack,
        },
        // Shift-tint check (change `shift-colour-module-ui`, task 3.2):
        // the composition fixture's B2.x buttons are Group2 members
        // (shift groups assign by controller number), so with Group2 active
        // their composed faces tint in the group token while Group1/Group3
        // faces and dimmed unaddressed cells render exactly as unshifted.
        Shot {
            file: "shift-colour.png",
            fixture: "fixtures/button_led_composition.ini",
            setup: setup_shift_colour,
        },
        // Switch-kind glyph check (change `s10-switch-glyphs`, task 3.2):
        // a bare S10 synthesizes the full 10-switch front whose geometry
        // annotates S1.1/S1.2 rotary and the rest toggle, so both glyph arms
        // read on one faceplate. Single controller, so the 2x stack zoom
        // fits with margin.
        Shot {
            file: "s10-switch-kinds.png",
            fixture: "fixtures/s10_switch_kinds.ini",
            setup: setup_faceplate_stack,
        },
        // Sections-mode lane check (change `sections-black-hole-solver`,
        // task 7.1): the smallest fixture carrying two comment banners, each
        // owning two circuits, rendered in the graph pane under
        // `LayoutMode::Sections`. Circuits chain in patch order into one
        // width-aware lane each; the two banner groups read as two contiguous
        // density bands on the single lane row (too few lanes to reach the
        // wrap target), which is what makes this the legible subject.
        Shot {
            file: "sections-lanes.png",
            fixture: "fixtures/graph_topology_error.ini",
            setup: setup_sections,
        },
        // Sections-mode scale check (same task): the 509-section scale anchor
        // in the graph pane under `LayoutMode::Sections`, where the new
        // lane/row arrangement replaced a 40:1 ribbon with a ~1.34 aspect
        // grid. Layout shape, not label text, is the claim: the shot proves
        // the big patch no longer collapses into one flat row.
        Shot {
            file: "sections-scale.png",
            fixture: "fixtures/own_scale.ini",
            setup: setup_sections,
        },
        // Force-arrangement check (change `force-arrangement-nonoverlap-and-
        // sliders`, task 4.3): the dependency-walk fixture in the graph pane
        // under `LayoutMode::Force` with the arrangement-aware `S` panel open.
        // The fixture is the legible subject for the non-overlap claim — it was
        // the second-worst pre-fix case (33 intersecting body pairs among its
        // dozen bodies on a 511-body scale anchor's heels) and small enough
        // that its bodies and labels stay readable at the fit zoom, unlike the
        // scale anchor whose whole world is in frame. The shot carries two
        // claims: no two drawn bodies intersect (the force arrangement's
        // non-overlap post-condition at the shipped defaults), and the panel's
        // three rows name the force knobs — Repulsion, Cohesion, Cable — where
        // the loading-time gate used to lay out nothing but the sections rows.
        Shot {
            file: "force-arrangement.png",
            fixture: "fixtures/graph_dependency_walk.ini",
            setup: setup_force_sliders,
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
    eprintln!(
        "  shots: module-ui, module-ui-labels, graph, source-viewer, optimizer, faceplate-rotary-stack, button-led-composition, led-register-colour, unreferenced-elements, shift-colour, s10-switch-kinds, sections-lanes, sections-scale, force-arrangement, large-startup, large-graph"
    );
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
        "module-ui-labels",
        "graph",
        "source-viewer",
        "optimizer",
        "faceplate-rotary-stack",
        "button-led-composition",
        "led-register-colour",
        "unreferenced-elements",
        "shift-colour",
        "s10-switch-kinds",
        "sections-lanes",
        "sections-scale",
        "force-arrangement",
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
