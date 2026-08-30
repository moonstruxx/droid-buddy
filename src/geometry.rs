use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::patch::Patch;

// ---------------------------------------------------------------------------
// Data model — mirrors rack_geometry.json (D1 schema)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RackGeometry {
    /// Unit of the coordinate system, e.g. "b32_pitch"
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
// Loader
// ---------------------------------------------------------------------------

impl RackGeometry {
    /// Load `rack_geometry.json` from the crate/repo root.
    ///
    /// Tries `CARGO_MANIFEST_DIR/rack_geometry.json` first (works for `cargo test`
    /// regardless of cwd), then `./rack_geometry.json` and `../rack_geometry.json`
    /// as fallbacks. Returns a descriptive `Err(String)` on failure and never
    /// panics.
    pub fn load() -> Result<Self, String> {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let candidates = [
            format!("{manifest_dir}/rack_geometry.json"),
            "rack_geometry.json".to_string(),
            "../rack_geometry.json".to_string(),
            "./rack_geometry.json".to_string(),
        ];
        let mut last_err = String::new();
        for cand in &candidates {
            let p = Path::new(cand);
            if !p.exists() {
                last_err = format!("not found: {cand}");
                continue;
            }
            match std::fs::read_to_string(p) {
                Ok(s) => match serde_json::from_str::<Self>(&s) {
                    Ok(v) => return Ok(v),
                    Err(e) => {
                        return Err(format!("failed to parse {cand}: {e}"));
                    }
                },
                Err(e) => {
                    last_err = format!("failed to read {cand}: {e}");
                }
            }
        }
        Err(format!(
            "rack_geometry.json not found or unreadable. Tried: {}. Last error: {last_err}",
            candidates.join(", ")
        ))
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
        let token = token.trim();
        if token.is_empty() {
            return None;
        }
        let mut chars = token.chars();
        let kind_raw = chars.next()?;
        if !kind_raw.is_ascii_alphabetic() {
            return None;
        }
        let kind = kind_raw.to_ascii_uppercase();

        // leading_number: skip 1 char, take digits (like patch.rs::leading_number)
        let digits: String = token
            .chars()
            .skip(1)
            .take_while(|c| c.is_ascii_digit())
            .collect();
        let instance: usize = digits.parse().unwrap_or(1);
        if instance == 0 {
            return None;
        }

        // element number: digits after '.' (1-based), fallback to 1 for singletons
        let element: u32 = if let Some(dot) = token.find('.') {
            token[dot + 1..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .unwrap_or(1)
        } else {
            1
        };
        if element == 0 {
            return None;
        }

        // Map token kind to grid key (case-insensitive). L is co-located with B.
        let grid_key = match kind {
            'B' | 'L' => "b32",
            'E' => "e4",
            'R' | 'M' | 'P' | 'O' | 'I' | 'S' | 'G' => "r2c",
            _ => return None,
        };

        // Find candidate controller slots whose grid matches (case-insensitive)
        let mut candidates: Vec<(&Rack, &ControllerSlot)> = Vec::new();
        for rack in &self.racks {
            for slot in &rack.controllers {
                if slot.grid.eq_ignore_ascii_case(grid_key) {
                    candidates.push((rack, slot));
                }
            }
        }
        if candidates.is_empty() {
            return None;
        }
        // Lookup grid (case-insensitive) before selecting slot so we can
        // handle singletons deterministically.
        let grid = self
            .grids
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(grid_key))
            .map(|(_, v)| v)?;
        // Singleton controllers (r2c) represent a single physical jack column
        // shared across all racks/instances. Cycling by instance would place
        // I1 on R1 and O4 on R2 (12 units apart) — a bogus distance for CV I/O
        // that physically sits on the same controller. Always pick the first
        // matching slot (R1's R2C) deterministically.
        let (rack, slot) = if matches!(grid, Grid::Singleton) {
            candidates[0]
        } else {
            let idx = (instance - 1) % candidates.len();
            candidates[idx]
        };

        let (off_x, off_y) = match grid {
            Grid::Matrix {
                cols,
                rows: _,
                row_wise,
                orientation: _,
            } => {
                let cols = *cols as u32;
                if *row_wise {
                    let col = (element - 1) % cols;
                    let row = (element - 1) / cols;
                    (col as i32, row as i32)
                } else {
                    // column-wise fallback (not used for B32)
                    let rows = match grid {
                        Grid::Matrix { rows, .. } => *rows as u32,
                        _ => 4,
                    };
                    let row = (element - 1) % rows;
                    let col = (element - 1) / rows;
                    (col as i32, row as i32)
                }
            }
            Grid::Stack { count: _, pitch_y } => {
                let row = (element - 1) as i32 * *pitch_y as i32;
                (0, row)
            }
            Grid::Singleton => (0, 0),
        };

        let abs_x = slot.x + off_x;
        let abs_y = rack.y + off_y;
        if !(0..=255).contains(&abs_x) || !(0..=255).contains(&abs_y) {
            return None;
        }
        Some((abs_x as u8, abs_y as u8))
    }

    /// Euclidean distance between two absolute positions in B32-grid units.
    pub fn distance(a: (u8, u8), b: (u8, u8)) -> f32 {
        let dx = a.0 as f32 - b.0 as f32;
        let dy = a.1 as f32 - b.1 as f32;
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

    /// Token-level adjacency.
    pub fn token_adjacent(&self, a: &str, b: &str) -> bool {
        match (self.resolve(a), self.resolve(b)) {
            (Some(pa), Some(pb)) => Self::is_adjacent(pa, pb),
            _ => false,
        }
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
        let euclidean = RackGeometry::distance(src_xy, sink_xy);
        let manhattan = {
            let dx = (src_xy.0 as i16 - sink_xy.0 as i16).unsigned_abs();
            let dy = (src_xy.1 as i16 - sink_xy.1 as i16).unsigned_abs();
            (dx + dy).min(255) as u8
        };
        let adjacent = RackGeometry::is_adjacent(src_xy, sink_xy);
        let (same_controller, same_rack) = controller_rack_flags(src_token, sink_token, geometry);
        let cable_hops = compute_cable_hops(patch, src_token, sink_token);
        Some(Self {
            src_kind: token_kind_u8(src_token),
            sink_kind: token_kind_u8(sink_token),
            param_key,
            src_xy,
            sink_xy,
            euclidean,
            manhattan,
            same_controller,
            same_rack,
            adjacent,
            cable_hops,
        })
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
        Some('R') => 8,
        Some('M') => 8,
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
        .take_while(|c| c.is_ascii_digit())
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

/// Embedded artifact (generated by tools/fit_outlier_model.py --seed 42).
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
        let m = f.manhattan as f32;
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

const HW_TOKEN_LETTERS: [char; 10] = ['B', 'L', 'P', 'O', 'I', 'E', 'S', 'M', 'R', 'G'];

fn scan_hw_tokens_local(value: &str) -> Vec<String> {
    let chars: Vec<char> = value.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let boundary_ok = i == 0 || !(chars[i - 1].is_ascii_alphanumeric() || chars[i - 1] == '_');
        let starts_token = HW_TOKEN_LETTERS.contains(&c)
            && i + 1 < chars.len()
            && chars[i + 1].is_ascii_digit()
            && boundary_ok;
        if starts_token {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i < chars.len()
                && chars[i] == '.'
                && i + 1 < chars.len()
                && chars[i + 1].is_ascii_digit()
            {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let clean_end = i >= chars.len()
                || !(chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.');
            if clean_end {
                tokens.push(chars[start..i].iter().collect());
            }
            continue;
        }
        i += 1;
    }
    tokens
}

fn scan_internal_tokens_local(value: &str) -> Vec<String> {
    let chars: Vec<char> = value.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '_' {
            let boundary_ok = i == 0 || !(chars[i - 1].is_ascii_alphanumeric());
            if boundary_ok {
                let start = i;
                i += 1;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                if i > start + 1 {
                    let token: String = chars[start..i].iter().collect();
                    let clean_end =
                        i >= chars.len() || !(chars[i].is_ascii_alphanumeric() || chars[i] == '_');
                    if clean_end {
                        out.push(token);
                    }
                }
                continue;
            }
        }
        i += 1;
    }
    out
}

fn section_contains_token(section: &crate::patch::IniSection, token: &str) -> bool {
    section
        .entries
        .iter()
        .any(|(_, v)| scan_hw_tokens_local(v).iter().any(|t| t == token))
}

fn section_consumes_cable(section: &crate::patch::IniSection, cable: &str) -> bool {
    section.entries.iter().any(|(k, v)| {
        let toks = scan_internal_tokens_local(v);
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
    // Build adjacency producer -> consumers via circuit_outputs + sink scan.
    let mut adj: HashMap<usize, Vec<usize>> = HashMap::new();
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
                    adj.entry(prod_idx).or_default().push(cons_idx);
                }
            }
        }
    }
    // Dedup adjacency lists for deterministic BFS.
    for v in adj.values_mut() {
        v.sort_unstable();
        v.dedup();
    }
    // BFS from all src sections.
    let mut queue: VecDeque<(usize, u8)> = VecDeque::new();
    let mut visited: HashSet<usize> = HashSet::new();
    for &s in &src_indices {
        queue.push_back((s, 0));
        visited.insert(s);
    }
    while let Some((node, dist)) = queue.pop_front() {
        if let Some(neigh) = adj.get(&node) {
            for &n in neigh {
                if visited.contains(&n) {
                    continue;
                }
                let next_dist = dist.saturating_add(1);
                if sink_indices.contains(&n) {
                    return next_dist;
                }
                visited.insert(n);
                queue.push_back((n, next_dist));
            }
        }
    }
    0
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
            .take_while(|c| c.is_ascii_digit())
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
        Some((abs.0 as i32 - slot.x, abs.1 as i32 - rack.y))
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
        assert!(geo.token_distance("L1.17", "B1.17").unwrap() == 0.0);
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
        // Validates the real committed file when present
        let loaded = RackGeometry::load();
        assert!(loaded.is_ok(), "load failed: {:?}", loaded.err());
        let geo = loaded.unwrap();
        assert!(!geo.racks.is_empty());
        assert!(geo.grids.contains_key("b32"));
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
        let dx = (feat.src_xy.0 as i16 - feat.sink_xy.0 as i16).unsigned_abs() as u8;
        let dy = (feat.src_xy.1 as i16 - feat.sink_xy.1 as i16).unsigned_abs() as u8;
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
