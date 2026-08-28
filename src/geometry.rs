use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

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
    /// element 17 → row 2 col 0.
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
        // Select by module instance (instance 1 → first candidate, 2 → second, wrap)
        let idx = (instance - 1) % candidates.len();
        let (rack, slot) = candidates[idx];

        // Lookup grid (case-insensitive)
        let grid = self
            .grids
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(grid_key))
            .map(|(_, v)| v)?;

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
            "b32":{"kind":"matrix","cols":8,"rows":4,"row_wise":true,"orientation":"vertical"},
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
    fn b1_17_resolves_to_row2_col0() {
        let geo = test_geometry();
        // Element 17 in a 4×8 row-wise matrix → col 0, row 2 (0-based)
        let off = offset_of(&geo, "B1.17").expect("B1.17 resolves");
        assert_eq!(
            off,
            (0, 2),
            "B1.17 should be row 2 col 0 within its B32 slot"
        );

        // Also check absolute via resolve: strip instance, index grid correctly
        // b32 lower-case variant must give same offset
        let off_lower = offset_of(&geo, "b1.17").expect("b1.17 resolves");
        assert_eq!(off_lower, (0, 2));
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
}
