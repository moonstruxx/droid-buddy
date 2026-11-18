use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::patch::Patch;

// ---------------------------------------------------------------------------
// Data model — mirrors rack_geometry.json (D1 schema)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RackGeometry {
    /// Unit of the coordinate system, e.g. "`b32_pitch`"
    pub unit: String,
    pub racks: Vec<Rack>,
    pub grids: HashMap<String, Grid>,
    /// Optional co-location map, e.g. {"L":"B"} — LEDs share button cell
    #[serde(default)]
    pub co_located: HashMap<String, String>,
    /// Optional shared-grid aliases (informational, case-insensitive lookup already)
    #[serde(default)]
    pub shared_grids: HashMap<String, String>,
    // unknown fields are ignored by serde default behaviour when not denied
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Rack {
    pub id: String,
    /// Y origin of this rack band in B32-grid units
    pub y: i32,
    pub controllers: Vec<ControllerSlot>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ControllerSlot {
    /// Human name, e.g. "B32" / "b32" / "E4" / "e4" / "R2C"
    pub name: String,
    /// X origin of this controller within its rack in B32-grid units
    pub x: i32,
    /// Grid key, e.g. "b32" / "e4" / "r2c" (case-insensitive)
    pub grid: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind")]
pub enum Grid {
    #[serde(rename = "matrix")]
    Matrix {
        cols: u8,
        rows: u8,
        #[serde(default = "default_true")]
        row_wise: bool,
        /// Optional orientation hint, e.g. "vertical" / "horizontal"
        #[serde(default)]
        orientation: Option<String>,
    },
    #[serde(rename = "stack")]
    Stack { count: u8, pitch_y: u8 },
    #[serde(rename = "singleton")]
    Singleton,
}

fn default_true() -> bool {
    true
}

// ---------------------------------------------------------------------------
// Token grammar — the parser behind `RackGeometry::resolve`
// ---------------------------------------------------------------------------

/// Parse a hardware token into `(kind letter, module instance, element)`.
///
/// Mirrors `HwComponent::module_instance()` / `leading_number()` in
/// `src/patch.rs`: the leading digit-run after the kind letter is the module
/// instance, the digits after the dot are the (1-based) element. Singletons
/// carry no digits and default to 1.
fn token_parts(token: &str) -> Option<(char, usize, u32)> {
    let kind = token.chars().next()?;
    if !kind.is_ascii_alphabetic() {
        return None;
    }
    let digits: String = token
        .chars()
        .skip(1)
        .take_while(char::is_ascii_digit)
        .collect();
    let instance: usize = digits.parse().unwrap_or(1);
    if instance == 0 {
        return None;
    }
    let element = dot_element(token);
    if element == 0 {
        return None;
    }
    Some((kind.to_ascii_uppercase(), instance, element))
}

/// The element number after the dot (1-based), `1` when the token has none.
fn dot_element(token: &str) -> u32 {
    let Some(dot) = token.find('.') else {
        return 1;
    };
    token[dot + 1..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap_or(1)
}

// ---------------------------------------------------------------------------
// Loader
// ---------------------------------------------------------------------------

/// Compiled in so a binary copied away from the repo still has its rack data.
const RACK_GEOMETRY_JSON: &str = include_str!("../rack_geometry.json");

impl RackGeometry {
    /// Parse the embedded `rack_geometry.json`.
    ///
    /// Malformed data returns a descriptive `Err(String)` so callers can warn
    /// and degrade; this never panics.
    pub fn load() -> Result<Self, String> {
        Self::from_json(RACK_GEOMETRY_JSON)
    }

    fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text)
            .map_err(|e| format!("failed to parse embedded rack_geometry.json: {e}"))
    }

    /// Resolve a hardware token like `B1.17`, `L1.17`, `E4.4`, `M4.2` to an
    /// absolute position in B32-grid units.
    ///
    /// Behaviour mirrors `HwComponent::module_instance()` / `leading_number()` in
    /// `src/patch.rs`: strip the leading digit-run (module instance) and use the
    /// element number after the dot to index the grid. For a 4×8 row-wise matrix
    /// element 17 → row 4 col 0.
    ///
    /// Co-located `L→B` pairs share the same cell (distance 0). Mirrored
    /// controller names (`B32`/`b32`, `E4`/`e4`) share the same element grid via
    /// case-insensitive grid-key resolution.
    pub fn resolve(&self, token: &str) -> Option<(u8, u8)> {
        let (kind, instance, element) = token_parts(token.trim())?;
        let grid_key = Self::grid_key_for_kind(kind)?;
        let grid = self.grid(grid_key)?;
        let (rack, slot) = self.slot_for(grid_key, instance, grid)?;
        let (off_x, off_y) = Self::grid_offset(grid, element);

        let abs_x = slot.x + off_x;
        let abs_y = rack.y + off_y;
        if !Self::in_bounds(abs_x, abs_y) {
            return None;
        }
        Some((abs_x as u8, abs_y as u8))
    }

    /// The grid key a token kind resolves on (case-insensitive). `L` is
    /// co-located with `B`; the single-column controllers share `r2c`.
    fn grid_key_for_kind(kind: char) -> Option<&'static str> {
        match kind {
            'B' | 'L' => Some("b32"),
            'E' => Some("e4"),
            'R' | 'M' | 'P' | 'O' | 'I' | 'S' | 'G' => Some("r2c"),
            _ => None,
        }
    }

    /// The grid declaration for `grid_key`, matched case-insensitively.
    fn grid(&self, grid_key: &str) -> Option<&Grid> {
        self.grids
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(grid_key))
            .map(|(_, grid)| grid)
    }

    /// Every controller slot whose grid matches, in rack then slot declaration
    /// order.
    fn candidate_slots<'a>(&'a self, grid_key: &str) -> Vec<(&'a Rack, &'a ControllerSlot)> {
        self.racks
            .iter()
            .flat_map(|rack| rack.controllers.iter().map(move |slot| (rack, slot)))
            .filter(|(_, slot)| slot.grid.eq_ignore_ascii_case(grid_key))
            .collect()
    }

    /// The `(rack, slot)` a token `instance` lands on for `grid`.
    ///
    /// Singleton controllers (`r2c`) represent a single physical jack column
    /// shared across all racks/instances. Cycling by instance would place I1
    /// on R1 and O4 on R2 (12 units apart) — a bogus distance for CV I/O that
    /// physically sits on the same controller — so they always pick the first
    /// matching slot (R1's R2C) deterministically. Matrix and stack grids
    /// cycle through their candidates by instance.
    fn slot_for(
        &self,
        grid_key: &str,
        instance: usize,
        grid: &Grid,
    ) -> Option<(&Rack, &ControllerSlot)> {
        let candidates = self.candidate_slots(grid_key);
        if matches!(grid, Grid::Singleton) {
            return candidates.first().copied();
        }
        if candidates.is_empty() {
            return None;
        }
        let index = (instance - 1) % candidates.len();
        Some(candidates[index])
    }

    /// The `(x, y)` offset of `element` within a controller grid, in B32-grid
    /// units. Rows run row-wise for `b32`; the column-wise fallback keeps a
    /// vertical orientation readable.
    fn grid_offset(grid: &Grid, element: u32) -> (i32, i32) {
        match grid {
            Grid::Matrix {
                cols,
                rows,
                row_wise,
                orientation: _,
            } => {
                let cols = u32::from(*cols);
                if *row_wise {
                    (((element - 1) % cols) as i32, ((element - 1) / cols) as i32)
                } else {
                    let rows = u32::from(*rows);
                    (((element - 1) / rows) as i32, ((element - 1) % rows) as i32)
                }
            }
            Grid::Stack { count: _, pitch_y } => (0, (element - 1) as i32 * i32::from(*pitch_y)),
            Grid::Singleton => (0, 0),
        }
    }

    /// Whether a grid coordinate is representable in a resolved position.
    fn in_bounds(x: i32, y: i32) -> bool {
        (0..=255).contains(&x) && (0..=255).contains(&y)
    }

    /// Euclidean distance between two absolute positions in B32-grid units.
    pub fn distance(a: (u8, u8), b: (u8, u8)) -> f32 {
        let dx = f32::from(a.0) - f32::from(b.0);
        let dy = f32::from(a.1) - f32::from(b.1);
        (dx * dx + dy * dy).sqrt()
    }

    /// Token-level Euclidean distance (resolves both tokens, returns None if
    /// either token is unknown).
    pub fn token_distance(&self, a: &str, b: &str) -> Option<f32> {
        let pa = self.resolve(a)?;
        let pb = self.resolve(b)?;
        Some(Self::distance(pa, pb))
    }

    /// Whether two absolute positions are adjacent (distance == 1, 4-neighbour).
    pub fn is_adjacent(a: (u8, u8), b: (u8, u8)) -> bool {
        let d = Self::distance(a, b);
        (d - 1.0).abs() < 1e-6
    }
}

// Free helpers for callers that already have positions
pub fn distance(a: (u8, u8), b: (u8, u8)) -> f32 {
    RackGeometry::distance(a, b)
}

pub fn is_adjacent(a: (u8, u8), b: (u8, u8)) -> bool {
    RackGeometry::is_adjacent(a, b)
}

// ---------------------------------------------------------------------------
// BindingFeatures — D3 shape (Track 1 + Track 2 shared)
// ---------------------------------------------------------------------------

/// Single feature struct that feeds both the hard invariant and the learned
/// spike (design D3). All distances are in B32-grid units.
#[derive(Debug, Clone, PartialEq)]
pub struct BindingFeatures {
    pub src_kind: u8,
    pub sink_kind: u8,
    pub param_key: u8,
    pub src_xy: (u8, u8),
    pub sink_xy: (u8, u8),
    pub euclidean: f32,
    pub manhattan: u8,
    pub same_controller: bool,
    pub same_rack: bool,
    pub adjacent: bool,
    pub cable_hops: u8,
}

impl BindingFeatures {
    /// Compute binding features for `src_token -> sink_token` in the context
    /// of `geometry` and `patch`.
    ///
    /// Returns `None` if either token cannot be resolved to a grid position.
    /// `param_key` is set to 0 (the binding API does not carry a separate
    /// param discriminant; callers needing per-param granularity can use
    /// `from_tokens_with_param`).
    pub fn from_tokens(
        src_token: &str,
        sink_token: &str,
        geometry: &RackGeometry,
        patch: &Patch,
    ) -> Option<Self> {
        Self::from_tokens_with_param(src_token, sink_token, 0, geometry, patch)
    }

    /// Like `from_tokens` but with an explicit `param_key` discriminant.
    pub fn from_tokens_with_param(
        src_token: &str,
        sink_token: &str,
        param_key: u8,
        geometry: &RackGeometry,
        patch: &Patch,
    ) -> Option<Self> {
        let src_xy = geometry.resolve(src_token)?;
        let sink_xy = geometry.resolve(sink_token)?;
        let (same_controller, same_rack) = controller_rack_flags(src_token, sink_token, geometry);
        let cable_hops = compute_cable_hops(patch, src_token, sink_token);
        Some(Self::from_parts(
            token_kind_u8(src_token),
            token_kind_u8(sink_token),
            param_key,
            src_xy,
            sink_xy,
            same_controller,
            same_rack,
            cable_hops,
        ))
    }

    /// Same result as `from_tokens_with_param`, but geometry resolution,
    /// controller flags and section containment come from a precomputed
    /// `WiringContext` shared by every pair in the same scan (O(1) per pair
    /// instead of two full section passes).
    pub(crate) fn from_tokens_with_context(
        src_token: &str,
        sink_token: &str,
        param_key: u8,
        ctx: &WiringContext,
    ) -> Option<Self> {
        let src_xy = ctx.resolved.get(src_token).copied()?;
        let sink_xy = ctx.resolved.get(sink_token).copied()?;
        let (same_controller, same_rack) = match (
            ctx.controller.get(src_token),
            ctx.controller.get(sink_token),
        ) {
            (Some((sr, ss, sx, sg)), Some((kr, ks, kx, kg))) => {
                let same_rack = sr == kr;
                let same_controller =
                    same_rack && ss == ks && sx == kx && sg.eq_ignore_ascii_case(kg);
                (same_controller, same_rack)
            }
            _ => (false, false),
        };
        let cable_hops = ctx.cable_hops(src_token, sink_token);
        Some(Self::from_parts(
            token_kind_u8(src_token),
            token_kind_u8(sink_token),
            param_key,
            src_xy,
            sink_xy,
            same_controller,
            same_rack,
            cable_hops,
        ))
    }

    /// Shared feature math so the direct and context-backed paths cannot
    /// drift: distances and adjacency derive from the resolved cells.
    #[allow(clippy::too_many_arguments)] // one arg per BindingFeatures field
    fn from_parts(
        src_kind: u8,
        sink_kind: u8,
        param_key: u8,
        src_xy: (u8, u8),
        sink_xy: (u8, u8),
        same_controller: bool,
        same_rack: bool,
        cable_hops: u8,
    ) -> Self {
        let euclidean = RackGeometry::distance(src_xy, sink_xy);
        let manhattan = {
            let dx = (i16::from(src_xy.0) - i16::from(sink_xy.0)).unsigned_abs();
            let dy = (i16::from(src_xy.1) - i16::from(sink_xy.1)).unsigned_abs();
            (dx + dy).min(255) as u8
        };
        let adjacent = RackGeometry::is_adjacent(src_xy, sink_xy);
        Self {
            src_kind,
            sink_kind,
            param_key,
            src_xy,
            sink_xy,
            euclidean,
            manhattan,
            same_controller,
            same_rack,
            adjacent,
            cable_hops,
        }
    }
}

pub(crate) fn token_kind_u8(token: &str) -> u8 {
    match token.chars().next().map(|c| c.to_ascii_uppercase()) {
        Some('B') => 0,
        Some('L') => 1,
        Some('P') => 2,
        Some('O') => 3,
        Some('I') => 4,
        Some('E') => 5,
        Some('S') => 6,
        Some('G') => 7,
        Some('R' | 'M') => 8,
        _ => 255,
    }
}

fn controller_rack_flags(
    src_token: &str,
    sink_token: &str,
    geometry: &RackGeometry,
) -> (bool, bool) {
    let src_info = controller_for_token(src_token, geometry);
    let sink_info = controller_for_token(sink_token, geometry);
    match (src_info, sink_info) {
        (Some((src_rack, src_slot)), Some((sink_rack, sink_slot))) => {
            let same_rack = src_rack.id == sink_rack.id;
            let same_controller = same_rack
                && src_slot.name == sink_slot.name
                && src_slot.x == sink_slot.x
                && src_slot.grid.eq_ignore_ascii_case(&sink_slot.grid);
            (same_controller, same_rack)
        }
        _ => (false, false),
    }
}

fn controller_for_token<'a>(
    token: &str,
    geometry: &'a RackGeometry,
) -> Option<(&'a Rack, &'a ControllerSlot)> {
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    let kind = token.chars().next()?.to_ascii_uppercase();
    let digits: String = token
        .chars()
        .skip(1)
        .take_while(char::is_ascii_digit)
        .collect();
    let instance: usize = digits.parse().unwrap_or(1);
    if instance == 0 {
        return None;
    }
    let grid_key = match kind {
        'B' | 'L' => "b32",
        'E' => "e4",
        'R' | 'M' | 'P' | 'O' | 'I' | 'S' | 'G' => "r2c",
        _ => return None,
    };
    let mut candidates: Vec<(&Rack, &ControllerSlot)> = Vec::new();
    for rack in &geometry.racks {
        for slot in &rack.controllers {
            if slot.grid.eq_ignore_ascii_case(grid_key) {
                candidates.push((rack, slot));
            }
        }
    }
    if candidates.is_empty() {
        return None;
    }
    // Singleton r2c controllers (I/O/P/S/G) are a single physical column;
    // do not cycle by instance — always resolve to the first matching slot
    // (deterministically R1's R2C) so I1/O4/P1.2 all co-locate.
    if grid_key.eq_ignore_ascii_case("r2c") {
        return Some(candidates[0]);
    }
    let idx = (instance - 1) % candidates.len();
    Some(candidates[idx])
}

// ---------------------------------------------------------------------------
// Wiring-outlier decision table — design D1/D2/D5 (learned, embedded)
// ---------------------------------------------------------------------------
// Embedded artifact fitted offline by tools/fit_outlier_model.py from
// corpus/features.csv (schema.rs include_str! precedent). Rows are evaluated
// top to bottom, first match wins; a row matching no rule falls back to the
// preserved threshold rule (euclidean > 8.0 && cable_hops == 0). The scorer is
// pure and deterministic — same patch, same verdict on every machine.

/// Fallback threshold preserved from the pre-table behavior (design D1).
const FALLBACK_EUCLIDEAN: f32 = 8.0;

/// Embedded artifact (generated by `tools/fit_outlier_model.py` --seed 42).
const OUTLIER_ARTIFACT: &str = include_str!("../tools/outlier_artifact.txt");

/// Verdict from the learned decision table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutlierVerdict {
    Flag,
    Pass,
}

#[derive(Debug, Clone, Copy)]
struct OutlierRule {
    min_euclidean: f32,
    max_euclidean: f32,
    min_manhattan: f32,
    max_manhattan: f32,
    /// `None` = '*' (any controller).
    same_controller: Option<u8>,
    /// `None` = '*' (any source kind).
    src_kind: Option<u8>,
    /// `None` = '*' (any sink kind).
    sink_kind: Option<u8>,
    verdict: OutlierVerdict,
}

impl OutlierRule {
    fn matches(&self, f: &BindingFeatures) -> bool {
        let e = f.euclidean;
        let m = f32::from(f.manhattan);
        let sc = u8::from(f.same_controller);
        self.min_euclidean <= e
            && e <= self.max_euclidean
            && self.min_manhattan <= m
            && m <= self.max_manhattan
            && self.same_controller.is_none_or(|v| v == sc)
            && self.src_kind.is_none_or(|v| v == f.src_kind)
            && self.sink_kind.is_none_or(|v| v == f.sink_kind)
    }
}

/// Learned wiring-outlier scorer over the embedded decision table (design
/// D1/D2). Pure, deterministic, no IO. Invariant guards stay at the call site
/// (design D5): adjacent / co-located / via-cable bindings never reach the
/// scorer — the artifact only ever sees scorer-visible rows.
#[derive(Debug)]
pub struct WiringOutlierScorer {
    rules: Vec<OutlierRule>,
}

impl Default for WiringOutlierScorer {
    /// Empty table → every row falls back to the threshold rule (graceful
    /// degradation, design D1: a broken/missing artifact never silences).
    fn default() -> Self {
        Self { rules: Vec::new() }
    }
}

impl WiringOutlierScorer {
    /// Parse the artifact text (8 whitespace-separated columns, '#' comments).
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut rules = Vec::new();
        for (idx, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() != 8 {
                return Err(format!(
                    "outlier artifact line {}: expected 8 columns, got {}",
                    idx + 1,
                    cols.len()
                ));
            }
            let num = |s: &str| -> Result<f32, String> {
                s.parse::<f32>()
                    .map_err(|_| format!("outlier artifact line {}: bad number {s:?}", idx + 1))
            };
            let cat = |s: &str| -> Result<Option<u8>, String> {
                if s == "*" {
                    Ok(None)
                } else {
                    s.parse::<u8>().map(Some).map_err(|_| {
                        format!("outlier artifact line {}: bad categorical {s:?}", idx + 1)
                    })
                }
            };
            let verdict = match cols[7] {
                "flag" => OutlierVerdict::Flag,
                "pass" => OutlierVerdict::Pass,
                other => {
                    return Err(format!(
                        "outlier artifact line {}: bad verdict {other:?}",
                        idx + 1
                    ))
                }
            };
            rules.push(OutlierRule {
                min_euclidean: num(cols[0])?,
                max_euclidean: num(cols[1])?,
                min_manhattan: num(cols[2])?,
                max_manhattan: num(cols[3])?,
                same_controller: cat(cols[4])?,
                src_kind: cat(cols[5])?,
                sink_kind: cat(cols[6])?,
                verdict,
            });
        }
        Ok(Self { rules })
    }

    /// The process-wide scorer over the embedded artifact. A parse failure
    /// degrades to the empty table → pure threshold fallback (never panics).
    pub fn embedded() -> &'static Self {
        use std::sync::OnceLock;
        static SCORER: OnceLock<WiringOutlierScorer> = OnceLock::new();
        SCORER.get_or_init(|| WiringOutlierScorer::parse(OUTLIER_ARTIFACT).unwrap_or_default())
    }

    /// Number of parsed rules (test aid).
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// First matching rule's verdict, or `None` when no rule matches (the
    /// caller/fallback path applies the threshold rule).
    pub fn verdict(&self, f: &BindingFeatures) -> Option<OutlierVerdict> {
        self.rules.iter().find(|r| r.matches(f)).map(|r| r.verdict)
    }

    /// Full decision for a scorer-visible binding: learned verdict first,
    /// threshold fallback on a miss (design D1).
    pub fn is_outlier(&self, f: &BindingFeatures) -> bool {
        match self.verdict(f) {
            Some(OutlierVerdict::Flag) => true,
            Some(OutlierVerdict::Pass) => false,
            None => f.euclidean > FALLBACK_EUCLIDEAN && f.cable_hops == 0,
        }
    }
}

// ---------------------------------------------------------------------------
// cable_hops — via Patch.cable_index + circuit_outputs graph traversal
// ---------------------------------------------------------------------------

fn section_contains_token(section: &crate::patch::IniSection, token: &str) -> bool {
    section.entries.iter().any(|(_, v)| {
        crate::patch::scan_hw_tokens_wide(v)
            .iter()
            .any(|t| t == token)
    })
}

fn section_consumes_cable(section: &crate::patch::IniSection, cable: &str) -> bool {
    section.entries.iter().any(|(k, v)| {
        let toks = crate::patch::scan_internal_tokens(v);
        if !toks.iter().any(|t| t == cable) {
            return false;
        }
        // Pure `output = _CABLE` is a source, not a sink.
        if k.to_lowercase() == "output" && v.trim() == cable {
            return false;
        }
        true
    })
}

/// Producer section → consumer sections for every cable the patch produces
/// (a consumer being any section with a non-`output` reference to that cable).
/// Adjacency lists are sorted and deduped so the BFS over them is
/// deterministic. Shared by the per-pair hop count and the precomputed
/// [`WiringContext`], so both paths see one adjacency definition.
fn consumer_adjacency(patch: &Patch) -> HashMap<usize, Vec<usize>> {
    let mut adjacency: HashMap<usize, Vec<usize>> = HashMap::new();
    for (prod_idx, outputs) in patch.circuit_outputs.iter().enumerate() {
        if outputs.is_empty() {
            continue;
        }
        for cable in outputs {
            for (cons_idx, section) in patch.sections.iter().enumerate() {
                if cons_idx == prod_idx {
                    continue;
                }
                if section_consumes_cable(section, cable) {
                    adjacency.entry(prod_idx).or_default().push(cons_idx);
                }
            }
        }
    }
    for v in adjacency.values_mut() {
        v.sort_unstable();
        v.dedup();
    }
    adjacency
}

/// Hop count from any source section to any sink section over the consumer
/// adjacency: 0 when no sink is reachable from the sources.
fn bfs_hops(
    adjacency: &HashMap<usize, Vec<usize>>,
    sources: &[usize],
    is_sink: impl Fn(usize) -> bool,
) -> u8 {
    let mut queue: VecDeque<(usize, u8)> = VecDeque::new();
    let mut visited: HashSet<usize> = HashSet::new();
    for &s in sources {
        queue.push_back((s, 0));
        visited.insert(s);
    }
    while let Some((node, dist)) = queue.pop_front() {
        let Some(neigh) = adjacency.get(&node) else {
            continue;
        };
        for &n in neigh {
            if visited.contains(&n) {
                continue;
            }
            let next_dist = dist.saturating_add(1);
            if is_sink(n) {
                return next_dist;
            }
            visited.insert(n);
            queue.push_back((n, next_dist));
        }
    }
    0
}

fn compute_cable_hops(patch: &Patch, src_token: &str, sink_token: &str) -> u8 {
    let src_indices: Vec<usize> = patch
        .sections
        .iter()
        .enumerate()
        .filter(|(_, s)| section_contains_token(s, src_token))
        .map(|(i, _)| i)
        .collect();
    let sink_indices: HashSet<usize> = patch
        .sections
        .iter()
        .enumerate()
        .filter(|(_, s)| section_contains_token(s, sink_token))
        .map(|(i, _)| i)
        .collect();

    if src_indices.is_empty() || sink_indices.is_empty() {
        return 0;
    }
    // Direct co-location in same section => 0 hops (no cable needed).
    if src_indices.iter().any(|i| sink_indices.contains(i)) {
        return 0;
    }
    bfs_hops(&consumer_adjacency(patch), &src_indices, |n| {
        sink_indices.contains(&n)
    })
}

/// Precomputed per-token lookups shared by every pair in one wiring-outlier
/// scan. The naive path (`BindingFeatures::from_tokens`) rescans every
/// section twice and re-resolves geometry per pair, which is quadratic on
/// controller sections (a [b32] section alone contributes 496 pairs from its
/// 32 button tokens) and dominated graph build on melody2 (~24 s release).
/// The scan builds this context once; every pair then costs O(1) lookups with
/// results identical to the naive path.
pub(crate) struct WiringContext {
    /// token -> section indices containing it (ascending, deduped); mirrors
    /// the per-pair collection in `compute_cable_hops`.
    token_sections: HashMap<String, Vec<usize>>,
    /// producer section -> consumer sections for every produced cable; built
    /// once instead of per pair.
    adjacency: HashMap<usize, Vec<usize>>,
    /// token -> resolved grid cell (only tokens the geometry can place).
    resolved: HashMap<String, (u8, u8)>,
    /// token -> (rack id, slot name, slot x, grid) for controller flags.
    controller: HashMap<String, (String, String, i32, String)>,
}

impl WiringContext {
    /// One pass over sections for containment and cable adjacency, then
    /// resolve/controller lookups per distinct token. `token_sections` uses
    /// `scan_hw_tokens_wide` so membership matches `section_contains_token`.
    pub(crate) fn new(patch: &Patch, geometry: &RackGeometry) -> Self {
        let mut token_sections: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, section) in patch.sections.iter().enumerate() {
            let mut section_seen: HashSet<String> = HashSet::new();
            for (_, value) in &section.entries {
                for tok in crate::patch::scan_hw_tokens_wide(value) {
                    if section_seen.insert(tok.clone()) {
                        token_sections.entry(tok).or_default().push(idx);
                    }
                }
            }
        }

        let adjacency = consumer_adjacency(patch);

        let mut resolved: HashMap<String, (u8, u8)> = HashMap::new();
        let mut controller: HashMap<String, (String, String, i32, String)> = HashMap::new();
        for token in token_sections.keys() {
            if let Some(xy) = geometry.resolve(token) {
                resolved.insert(token.clone(), xy);
            }
            if let Some((rack, slot)) = controller_for_token(token, geometry) {
                controller.insert(
                    token.clone(),
                    (
                        rack.id.clone(),
                        slot.name.clone(),
                        slot.x,
                        slot.grid.clone(),
                    ),
                );
            }
        }

        Self {
            token_sections,
            adjacency,
            resolved,
            controller,
        }
    }

    /// Hop count identical to `compute_cable_hops` for the same pair.
    fn cable_hops(&self, src_token: &str, sink_token: &str) -> u8 {
        let (Some(src_indices), Some(sink_indices)) = (
            self.token_sections.get(src_token),
            self.token_sections.get(sink_token),
        ) else {
            return 0;
        };
        if src_indices.is_empty() || sink_indices.is_empty() {
            return 0;
        }
        if src_indices.iter().any(|i| sink_indices.contains(i)) {
            return 0;
        }
        bfs_hops(&self.adjacency, src_indices, |n| sink_indices.contains(&n))
    }
}

// ---------------------------------------------------------------------------
// Tests — three scenarios from the spec
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_geometry() -> RackGeometry {
        // Prefer real file so the committed table is validated; fall back to
        // inline fixture for isolated runs.
        RackGeometry::load().unwrap_or_else(|_| inline_fixture())
    }

    fn inline_fixture() -> RackGeometry {
        let json = r#"{
          "unit":"b32_pitch",
          "racks":[
            {"id":"R1","y":0,"controllers":[{"name":"R2C","x":0,"grid":"r2c"},{"name":"E4","x":14,"grid":"e4"},{"name":"B32","x":30,"grid":"b32"}]},
            {"id":"R2","y":12,"controllers":[{"name":"R2C","x":0,"grid":"r2c"},{"name":"e4","x":14,"grid":"e4"},{"name":"b32","x":30,"grid":"b32"}]}
          ],
          "grids":{
            "b32":{"kind":"matrix","cols":4,"rows":8,"row_wise":true,"orientation":"vertical"},
            "e4":{"kind":"stack","count":4,"pitch_y":2},
            "r2c":{"kind":"singleton"}
          },
          "co_located":{"L":"B"},
          "shared_grids":{"B32":"b32","b32":"b32","E4":"e4","e4":"e4"}
        }"#;
        serde_json::from_str(json).expect("inline fixture parses")
    }

    /// Helper: offset of a resolved position relative to its controller origin.
    fn offset_of(geo: &RackGeometry, token: &str) -> Option<(i32, i32)> {
        let abs = geo.resolve(token)?;
        // find the same slot selection logic to get origin
        let kind = token.chars().next()?.to_ascii_uppercase();
        let digits: String = token
            .chars()
            .skip(1)
            .take_while(char::is_ascii_digit)
            .collect();
        let instance: usize = digits.parse().unwrap_or(1);
        let grid_key = match kind {
            'B' | 'L' => "b32",
            'E' => "e4",
            _ => "r2c",
        };
        let mut cands = Vec::new();
        for rack in &geo.racks {
            for slot in &rack.controllers {
                if slot.grid.eq_ignore_ascii_case(grid_key) {
                    cands.push((rack, slot));
                }
            }
        }
        let idx = (instance - 1) % cands.len();
        let (rack, slot) = cands[idx];
        Some((i32::from(abs.0) - slot.x, i32::from(abs.1) - rack.y))
    }

    #[test]
    fn b1_17_resolves_to_row4_col0() {
        let geo = test_geometry();
        // Element 17 in a 4×8 row-wise matrix → col 0, row 4 (0-based)
        let off = offset_of(&geo, "B1.17").expect("B1.17 resolves");
        assert_eq!(
            off,
            (0, 4),
            "B1.17 should be row 4 col 0 within its B32 slot"
        );

        // Also check absolute via resolve: strip instance, index grid correctly
        // b32 lower-case variant must give same offset
        let off_lower = offset_of(&geo, "b1.17").expect("b1.17 resolves");
        assert_eq!(off_lower, (0, 4));
    }

    #[test]
    fn co_located_led_button_distance_zero() {
        let geo = test_geometry();
        let a = geo.resolve("L1.17").expect("L1.17 resolves");
        let b = geo.resolve("B1.17").expect("B1.17 resolves");
        assert_eq!(a, b, "L1.17 and B1.17 must be co-located (same cell)");
        let d = RackGeometry::distance(a, b);
        assert!(d.abs() < 1e-6, "co-located L→B distance must be 0, got {d}");
        // token helper
        let td = geo.token_distance("L1.17", "B1.17").unwrap();
        assert!(td.abs() < 1e-6);
        assert_eq!(geo.token_distance("L1.17", "B1.17").unwrap(), 0.0);
    }

    #[test]
    fn shared_grids_for_mirrored_names() {
        let geo = test_geometry();
        // Grid lookup is case-insensitive: b32 and B32 reference the same grid
        assert!(geo.grids.contains_key("b32"));
        // Controller slots B32 and b32 both point to grid b32 (case-insensitive)
        let has_upper = geo
            .racks
            .iter()
            .flat_map(|r| &r.controllers)
            .any(|c| c.name == "B32" && c.grid.eq_ignore_ascii_case("b32"));
        let has_lower = geo
            .racks
            .iter()
            .flat_map(|r| &r.controllers)
            .any(|c| c.name == "b32" && c.grid.eq_ignore_ascii_case("b32"));
        assert!(has_upper && has_lower, "both B32 and b32 slots must exist");

        // E4 / e4 same grid
        let has_e_upper = geo
            .racks
            .iter()
            .flat_map(|r| &r.controllers)
            .any(|c| c.name == "E4" && c.grid.eq_ignore_ascii_case("e4"));
        let has_e_lower = geo
            .racks
            .iter()
            .flat_map(|r| &r.controllers)
            .any(|c| c.name == "e4" && c.grid.eq_ignore_ascii_case("e4"));
        assert!(
            has_e_upper && has_e_lower,
            "both E4 and e4 slots must exist"
        );

        // Token resolution through same grid: B and L share b32, E shares e4
        let b_pos = geo.resolve("B1.1").unwrap();
        let l_pos = geo.resolve("L1.1").unwrap();
        assert_eq!(b_pos, l_pos);

        // E4 element 4 should be at pitch_y*3 = 6 offset within its stack
        let e_off = offset_of(&geo, "E1.4").unwrap();
        assert_eq!(e_off, (0, 6));
        let e_off_lower = offset_of(&geo, "e1.4").unwrap();
        assert_eq!(e_off_lower, (0, 6));
    }

    #[test]
    fn handles_m4_style_tokens() {
        let geo = test_geometry();
        // M4.2-style tokens (fallback to singleton) must not panic and must resolve
        assert!(geo.resolve("M4.2").is_some());
        assert!(geo.resolve("B1.1").is_some());
    }

    #[test]
    fn distance_and_adjacent_helpers() {
        let a = (30u8, 2u8);
        let b = (31u8, 2u8);
        assert!((RackGeometry::distance(a, b) - 1.0).abs() < 1e-6);
        assert!(RackGeometry::is_adjacent(a, b));
        assert!(!RackGeometry::is_adjacent(a, (30, 2)));
        assert!(!RackGeometry::is_adjacent(a, (32, 2))); // distance 2
    }

    #[test]
    fn load_returns_ok() {
        // Validates the embedded data
        let loaded = RackGeometry::load();
        assert!(loaded.is_ok(), "load failed: {:?}", loaded.err());
        let geo = loaded.unwrap();
        assert!(!geo.racks.is_empty());
        assert!(geo.grids.contains_key("b32"));
    }

    #[test]
    fn embedded_rack_geometry_parses_and_matches_tracked_file() {
        let embedded = RackGeometry::load().expect("embedded rack_geometry.json parses");
        assert!(!embedded.racks.is_empty());

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("rack_geometry.json");
        let tracked = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("tracked {} unreadable: {e}", path.display()));
        assert_eq!(
            RACK_GEOMETRY_JSON, tracked,
            "compiled-in rack geometry drifted from the tracked file"
        );

        let from_file =
            RackGeometry::from_json(&tracked).expect("tracked rack_geometry.json parses");
        assert_eq!(
            serde_json::to_value(&embedded).expect("embedded geometry serialises"),
            serde_json::to_value(&from_file).expect("tracked geometry serialises"),
            "embedded and tracked rack geometry parse to different models"
        );
    }

    #[test]
    fn loader_does_not_search_the_filesystem() {
        // Production source is everything before the test module.
        let source = include_str!("geometry.rs");
        let production = source.split("#[cfg(test)]").next().expect("test module");
        assert!(
            !production.contains("CARGO_MANIFEST_DIR"),
            "rack geometry must come from include_str!, not a manifest-relative path"
        );
        assert!(
            !production.contains("std::fs"),
            "rack geometry must not be read from the filesystem at runtime"
        );
    }

    #[test]
    fn b32_grid_agrees_between_geometry_files() {
        // Regression (droid_tui-9xr): rack_geometry.json and
        // controller_geometry.json must agree that the B32 button grid is
        // 4 columns × 8 rows (manual orientation; physical.rs already asserts
        // the controller_geometry side, this ties the rack side to it).
        let rack = RackGeometry::load().expect("rack_geometry.json loads");
        let b32_grid = rack
            .grids
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("b32"))
            .map(|(_, v)| v)
            .expect("rack_geometry.json must carry a b32 grid");
        match b32_grid {
            Grid::Matrix { cols, rows, .. } => {
                assert_eq!(*cols, 4, "rack_geometry.json b32 cols must be 4");
                assert_eq!(*rows, 8, "rack_geometry.json b32 rows must be 8");
            }
            other => panic!("b32 must be a matrix grid, got {other:?}"),
        }

        let ctrl =
            crate::physical::ControllerGeometry::load().expect("controller_geometry.json loads");
        let b32 = ctrl
            .controller("b32")
            .expect("controller_geometry.json must carry a b32 entry");
        let buttons = &b32.element_cells["B"];
        let cols: std::collections::BTreeSet<u32> = buttons.iter().map(|c| c.col).collect();
        let rows: std::collections::BTreeSet<u32> = buttons.iter().map(|c| c.row).collect();
        assert_eq!(
            buttons.len(),
            32,
            "controller_geometry.json b32 must carry 32 buttons"
        );
        assert_eq!(
            cols.len(),
            4,
            "controller_geometry.json b32 must be 4 columns"
        );
        assert_eq!(rows.len(), 8, "controller_geometry.json b32 must be 8 rows");
        // Element 17 lands on row 4 col 0 in both models (4-col row-wise).
        assert!(rows.contains(&4), "b32 must span row 4 (B1.17..B1.20)");
    }

    /// Trailing digit-run of a label (`"E1.4"` -> 4, `"E1"` -> 1).
    fn trailing_number(label: &str) -> u32 {
        let digits: String = label
            .chars()
            .rev()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        digits.parse().expect("element label must end in a number")
    }

    #[test]
    fn e4_grid_agrees_between_geometry_files() {
        // Regression (controller-geometry-fidelity, task 1.1): rack_geometry.json
        // declares the E4 encoders as a vertical stack of 4; controller_geometry
        // .json's e4 entry must therefore be one column × four rows with
        // elements 1..=4 — mirroring the B32 4×8 agreement test above.
        let rack = RackGeometry::load().expect("rack_geometry.json loads");
        let e4_grid = rack
            .grids
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("e4"))
            .map(|(_, v)| v)
            .expect("rack_geometry.json must carry an e4 grid");
        match e4_grid {
            Grid::Stack { count, .. } => assert_eq!(*count, 4, "e4 stack count must be 4"),
            other => panic!("e4 must be a stack grid, got {other:?}"),
        }

        let ctrl =
            crate::physical::ControllerGeometry::load().expect("controller_geometry.json loads");
        let e4 = ctrl
            .controller("e4")
            .expect("controller_geometry.json must carry an e4 entry");
        let encoders = &e4.element_cells["E"];
        assert_eq!(
            encoders.len(),
            4,
            "controller_geometry.json e4 must carry 4 encoders"
        );
        let cols: std::collections::BTreeSet<u32> = encoders.iter().map(|c| c.col).collect();
        let rows: std::collections::BTreeSet<u32> = encoders.iter().map(|c| c.row).collect();
        assert_eq!(cols.len(), 1, "e4 must be a single column");
        assert_eq!(rows.len(), 4, "e4 must be four rows");

        // Elements 1..=4, with row index increasing as the element number
        // grows (a vertical stack).
        let mut by_element: Vec<(u32, u32, u32)> = encoders
            .iter()
            .map(|c| (trailing_number(&c.label), c.col, c.row))
            .collect();
        by_element.sort_unstable();
        assert_eq!(
            by_element.iter().map(|(e, _, _)| *e).collect::<Vec<_>>(),
            vec![1, 2, 3, 4],
            "e4 elements must be 1..=4"
        );
        for w in by_element.windows(2) {
            assert!(
                w[0].2 < w[1].2,
                "e4 rows must increase with element number ({} vs {})",
                w[0].2,
                w[1].2
            );
        }
    }

    // ---- Cross-file / cross-declaration grid agreement (droid_tui-46x) ----

    /// Mirror of the per-controller `grids` declarations in
    /// `controller_geometry.json`. The production model ignores that field (it
    /// is documentation no code reads), so the agreement sweep below reads it
    /// through this mirror; unknown fields are skipped by serde.
    #[derive(Deserialize)]
    struct ControllerGridDeclarations {
        controllers: HashMap<String, ControllerGridEntryMirror>,
    }

    #[derive(Deserialize)]
    struct ControllerGridEntryMirror {
        #[serde(default)]
        grids: Vec<GridDeclarationMirror>,
    }

    #[derive(Deserialize)]
    struct GridDeclarationMirror {
        family: String,
        cols: u32,
        rows: u32,
    }

    fn controller_grid_declarations() -> ControllerGridDeclarations {
        serde_json::from_str(crate::physical::CONTROLLER_GEOMETRY_JSON)
            .expect("controller_geometry.json parses")
    }

    #[test]
    fn every_declared_grid_agrees_with_element_cells() {
        // Regression (droid_tui-46x): every grid a controller declares in
        // controller_geometry.json must agree with the element cells the file
        // emits for the same family — count, column/row cardinality, and slot
        // distinctness — so a stack-vs-matrix drift like E4's four-cell row is
        // caught for every controller in the suite, not only where a
        // hand-written cross-file tie exists (b32, e4). Families that carry
        // cells but declare no grid (the E4 and DB8E LED rings) are shape-free
        // by design and are skipped. A family with several grids (s10/p10
        // rotary banks, master18's CV jacks) partitions its cells in list
        // order, one segment per grid extent, which is how the generator
        // emits them; each segment must then fill its declared extent.
        let ctrl =
            crate::physical::ControllerGeometry::load().expect("controller_geometry.json loads");
        let decls = controller_grid_declarations();
        let mut controllers_checked = 0usize;
        for (name, entry) in &decls.controllers {
            let geo = ctrl
                .controller(name)
                .unwrap_or_else(|| panic!("controller_geometry.json carries no {name} entry"));
            controllers_checked += 1;
            let families: std::collections::BTreeSet<&str> =
                entry.grids.iter().map(|g| g.family.as_str()).collect();
            for family in families.iter().copied() {
                let grids: Vec<&GridDeclarationMirror> =
                    entry.grids.iter().filter(|g| g.family == family).collect();
                let cells = geo.element_cells.get(family).unwrap_or_else(|| {
                    panic!("{name}: grid declares family {family} but no cells exist")
                });
                let slots: usize = grids.iter().map(|g| (g.cols * g.rows) as usize).sum();
                assert_eq!(
                    cells.len(),
                    slots,
                    "{name}: family {family} has {} cells but its grids declare {slots} slots",
                    cells.len()
                );
                // Partition the family's cells in list order, one segment per
                // grid extent (the generator's emission order).
                let mut idx = 0usize;
                for grid in &grids {
                    let seg = &cells[idx..idx + (grid.cols * grid.rows) as usize];
                    idx += seg.len();
                    let cols: std::collections::BTreeSet<u32> = seg.iter().map(|c| c.col).collect();
                    let rows: std::collections::BTreeSet<u32> = seg.iter().map(|c| c.row).collect();
                    assert_eq!(
                        cols.len(),
                        grid.cols as usize,
                        "{name}: {family} grid {}x{} must use {} distinct columns, got {}",
                        grid.cols,
                        grid.rows,
                        grid.cols,
                        cols.len()
                    );
                    assert_eq!(
                        rows.len(),
                        grid.rows as usize,
                        "{name}: {family} grid {}x{} must use {} distinct rows, got {}",
                        grid.cols,
                        grid.rows,
                        grid.rows,
                        rows.len()
                    );
                    let used: std::collections::BTreeSet<(u32, u32)> =
                        seg.iter().map(|c| (c.col, c.row)).collect();
                    assert_eq!(
                        used.len(),
                        seg.len(),
                        "{name}: {family} grid {}x{} must not overlap its own slots",
                        grid.cols,
                        grid.rows
                    );
                }
            }
        }
        assert!(
            controllers_checked >= 15,
            "sweep covered only {controllers_checked} controllers; controller_geometry.json carries 15"
        );
    }

    #[test]
    fn every_rack_grid_agrees_between_geometry_files() {
        // Regression (droid_tui-46x): generalized from the hand-written b32
        // and e4 ties — every grid rack_geometry.json describes for a
        // controller must appear among the grid declarations
        // controller_geometry.json makes for that controller, so a
        // stack-vs-matrix drift like E4's four-cell row fails the suite for
        // every controller the rack file describes, present or future.
        // Singleton grids carry no shape and are skipped; a stack is one
        // column, so it ties as 1 x count. The declared-grid side is itself
        // tied to the emitted cells by
        // every_declared_grid_agrees_with_element_cells above, so the two
        // sweeps compose rack shape -> declaration -> cells.
        let rack = RackGeometry::load().expect("rack_geometry.json loads");
        let decls = controller_grid_declarations();
        let mut described = 0usize;
        for (name, grid) in &rack.grids {
            let (cols, rows) = match grid {
                Grid::Singleton => continue,
                Grid::Matrix { cols, rows, .. } => (u32::from(*cols), u32::from(*rows)),
                Grid::Stack { count, .. } => (1, u32::from(*count)),
            };
            described += 1;
            let entry = decls
                .controllers
                .get(&name.to_lowercase())
                .unwrap_or_else(|| {
                    panic!(
                        "rack_geometry.json describes {name} but controller_geometry.json carries no {name} entry"
                    )
                });
            assert!(
                entry.grids.iter().any(|g| g.cols == cols && g.rows == rows),
                "rack_geometry.json describes {name} as {cols}x{rows}, but controller_geometry.json declares only {:?}",
                entry
                    .grids
                    .iter()
                    .map(|g| (g.family.as_str(), g.cols, g.rows))
                    .collect::<Vec<_>>()
            );
        }
        assert!(
            described >= 2,
            "rack_geometry.json must still describe the b32 and e4 grids (found {described})"
        );
    }

    // ---- BindingFeatures scenario tests (task 1.2) ----

    #[test]
    fn binding_features_far_direct_wire_e4_4_to_m4_2() {
        let geo = test_geometry();
        // Direct wire: src and sink in same section, no cable hops.
        let content = "[p2b8]\n[copy]\n    src = E4.4\n    dst = M4.2\n";
        let patch = crate::patch::Patch::from_ini_str(content, String::from("far_direct"))
            .expect("patch parses");
        let feat = BindingFeatures::from_tokens("E4.4", "M4.2", &geo, &patch)
            .expect("both tokens resolve");
        // Large distance across rack.
        assert!(
            feat.euclidean > 8.0,
            "far wire E4.4->M4.2 should be large, got {}",
            feat.euclidean
        );
        assert_eq!(feat.cable_hops, 0, "direct wire must have 0 cable hops");
        assert!(!feat.adjacent);
        assert!(!feat.same_controller);
        // src_xy and sink_xy must match geometry resolve.
        assert_eq!(feat.src_xy, geo.resolve("E4.4").unwrap());
        assert_eq!(feat.sink_xy, geo.resolve("M4.2").unwrap());
        // Manhattan should be |dx|+|dy|.
        let dx = (i16::from(feat.src_xy.0) - i16::from(feat.sink_xy.0)).unsigned_abs() as u8;
        let dy = (i16::from(feat.src_xy.1) - i16::from(feat.sink_xy.1)).unsigned_abs() as u8;
        assert_eq!(feat.manhattan, dx + dy);
        // Kind encoding.
        assert_eq!(feat.src_kind, token_kind_u8("E4.4"));
        assert_eq!(feat.sink_kind, token_kind_u8("M4.2"));
    }

    #[test]
    fn binding_features_adjacent_pair_b1_17_b1_18() {
        let geo = test_geometry();
        let content = "[p2b8]\n[copy]\n    a = B1.17\n    b = B1.18\n";
        let patch = crate::patch::Patch::from_ini_str(content, String::from("adjacent"))
            .expect("patch parses");
        let feat = BindingFeatures::from_tokens("B1.17", "B1.18", &geo, &patch)
            .expect("both tokens resolve");
        assert!(
            (feat.euclidean - 1.0).abs() < 1e-6,
            "adjacent B1.17->B1.18 distance 1, got {}",
            feat.euclidean
        );
        assert_eq!(feat.manhattan, 1);
        assert!(feat.adjacent, "B1.17 and B1.18 should be adjacent");
        assert!(feat.same_controller, "same B32 controller");
        assert!(feat.same_rack, "same rack");
        assert_eq!(feat.cable_hops, 0);
    }

    #[test]
    fn binding_features_via_cable_pair() {
        let geo = test_geometry();
        // Distant target reached through one cable hop.
        let content = "[p2b8]\n\
             [src]\n    output = _WIRE\n    src = E4.4\n\
             [sink]\n    input = _WIRE\n    dst = M4.2\n";
        let patch = crate::patch::Patch::from_ini_str(content, String::from("via_cable"))
            .expect("patch parses");
        let feat = BindingFeatures::from_tokens("E4.4", "M4.2", &geo, &patch)
            .expect("both tokens resolve");
        assert!(
            feat.euclidean > 8.0,
            "via-cable pair should still be far, got {}",
            feat.euclidean
        );
        assert!(
            feat.cable_hops > 0,
            "via-cable pair must have cable_hops>0, got {}",
            feat.cable_hops
        );
        assert_eq!(feat.cable_hops, 1);
        // Adjacent false for far pair.
        assert!(!feat.adjacent);
    }

    #[test]
    fn binding_features_via_cable_two_hops() {
        let geo = test_geometry();
        let content = "[p2b8]\n\
             [src]\n    output = _A\n    src = E4.4\n\
             [mid]\n    input = _A\n    output = _B\n\
             [sink]\n    input = _B\n    dst = M4.2\n";
        let patch = crate::patch::Patch::from_ini_str(content, String::from("two_hops"))
            .expect("patch parses");
        let feat = BindingFeatures::from_tokens("E4.4", "M4.2", &geo, &patch)
            .expect("both tokens resolve");
        assert_eq!(feat.cable_hops, 2, "two cable hops via _A -> _B");
    }

    #[test]
    fn binding_features_none_when_token_unresolvable() {
        let geo = test_geometry();
        let content = "[p2b8]\n[copy]\n    a = B1.1\n";
        let patch = crate::patch::Patch::from_ini_str(content, String::from("t")).unwrap();
        assert!(BindingFeatures::from_tokens("B1.1", "ZZ99", &geo, &patch).is_none());
        assert!(BindingFeatures::from_tokens("ZZ99", "B1.1", &geo, &patch).is_none());
    }

    #[test]
    fn wiring_context_matches_naive_path() {
        // The precomputed WiringContext must never drift from the naive
        // per-pair path: same features for direct, adjacent, via-cable and
        // unresolvable pairs.
        let geo = test_geometry();
        let content = "[p2b8]\n\
             [copy]\n    src = E4.4\n    dst = M4.2\n\
             [mid]\n    input = _A\n    output = _B\n\
             [src]\n    output = _A\n    src = B1.1\n\
             [sink]\n    input = _B\n    dst = M4.2\n\
             [b32]\n    a = B1.17\n    b = B1.18\n";
        let patch = crate::patch::Patch::from_ini_str(content, String::from("ctx_match"))
            .expect("patch parses");
        let ctx = WiringContext::new(&patch, &geo);
        let pairs = [
            ("E4.4", "M4.2"),   // far direct wire, co-located in [copy]
            ("B1.17", "B1.18"), // adjacent, same controller
            ("B1.1", "M4.2"),   // via cable, different sections (BFS path)
            ("B1.1", "B1.1"),   // identical token
            ("ZZ99", "B1.1"),   // unresolvable -> both None
            ("B1.1", "ZZ99"),
        ];
        for (a, b) in pairs {
            let naive = BindingFeatures::from_tokens(a, b, &geo, &patch);
            let context = BindingFeatures::from_tokens_with_context(a, b, 0, &ctx);
            assert_eq!(
                context, naive,
                "context path diverges from naive path for {a} -> {b}"
            );
        }
        // The via-cable pair must actually exercise the BFS (hops > 0).
        let via = BindingFeatures::from_tokens("B1.1", "M4.2", &geo, &patch).unwrap();
        assert!(via.cable_hops > 0, "B1.1 -> M4.2 should route via cable");
    }

    #[test]
    fn binding_features_co_located_led_button() {
        let geo = test_geometry();
        let content = "[p2b8]\n[copy]\n    a = L1.1\n    b = B1.1\n";
        let patch = crate::patch::Patch::from_ini_str(content, String::from("colo")).unwrap();
        let feat = BindingFeatures::from_tokens("L1.1", "B1.1", &geo, &patch).unwrap();
        assert!((feat.euclidean).abs() < 1e-6);
        assert_eq!(feat.manhattan, 0);
        assert!(!feat.adjacent); // distance 0 is not adjacent (distance 1)
        assert_eq!(feat.src_xy, feat.sink_xy);
    }

    // ---- WiringOutlierScorer tests (task 2.1) ----

    #[test]
    fn embedded_artifact_parses_to_32_rules() {
        // The committed artifact must stay parseable and complete; a parse
        // failure would degrade the scorer to the pure threshold fallback.
        let scorer = WiringOutlierScorer::embedded();
        assert_eq!(scorer.rule_count(), 32, "artifact must keep 32 rules");
    }

    #[test]
    fn scorer_flags_learned_outlier() {
        // E4.4 -> M4.2 is a learned flag row (`15.1815..15.2815 * 5 8 flag`):
        // the E=5/M=8 kind pair is flagged within the fitted euclidean window
        // (the true E4.4->M4.2 distance sqrt(14^2 + 6^2) = 15.232).
        let scorer = WiringOutlierScorer::embedded();
        let f = BindingFeatures {
            src_kind: 5,
            sink_kind: 8,
            param_key: 0,
            src_xy: (0, 0),
            sink_xy: (14, 6),
            euclidean: 15.232,
            manhattan: 20,
            same_controller: false,
            same_rack: true,
            adjacent: false,
            cable_hops: 0,
        };
        assert_eq!(scorer.verdict(&f), Some(OutlierVerdict::Flag));
        assert!(scorer.is_outlier(&f));
    }

    #[test]
    fn scorer_fallback_flags_on_table_miss() {
        // src_kind 3 (O) -> sink_kind 8 (M) has no table row; the preserved
        // threshold rule must catch the far direct binding (design D1).
        let scorer = WiringOutlierScorer::embedded();
        let f = BindingFeatures {
            src_kind: 3,
            sink_kind: 8,
            param_key: 0,
            src_xy: (0, 0),
            sink_xy: (40, 40),
            euclidean: 56.6,
            manhattan: 80,
            same_controller: false,
            same_rack: true,
            adjacent: false,
            cable_hops: 0,
        };
        assert_eq!(scorer.verdict(&f), None, "no rule matches O->M");
        assert!(scorer.is_outlier(&f), "fallback threshold must flag it");
    }

    #[test]
    fn scorer_fallback_passes_near_binding_on_table_miss() {
        let scorer = WiringOutlierScorer::embedded();
        let f = BindingFeatures {
            src_kind: 3,
            sink_kind: 8,
            param_key: 0,
            src_xy: (0, 0),
            sink_xy: (2, 2),
            euclidean: 2.8,
            manhattan: 4,
            same_controller: false,
            same_rack: true,
            adjacent: false,
            cable_hops: 0,
        };
        assert_eq!(scorer.verdict(&f), None);
        assert!(
            !scorer.is_outlier(&f),
            "near binding must pass the fallback"
        );
    }
}
