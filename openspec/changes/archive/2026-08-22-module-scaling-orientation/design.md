## Context

The DROID patch viewer renders hardware components from `.ini` patch files in a terminal UI built with ratatui 0.29. The current layout is fixed with no mechanism for scaling or reorienting components. This design adds module scaling and orientation support to improve adaptability to different display configurations and hardware revisions.

Key constraints:
- Single-threaded event loop, no async runtime
- Renderer owns layout; component_rects published per-frame for mouse hit-testing
- Fresh layout recomputed each draw - resize is a no-op
- Custom .ini parser with boundary-aware token scanner
- Shift groups as enum with color/key_label methods
- 16×2 component cell grid, header 3 rows, status 3 rows

## Goals / Non-Goals

**Goals:**
- Add configurable component scaling presets (50%, 100%, 150%, 200%)
- Add orientation switching between portrait and landscape
- Update renderer to handle scale/orientation transformations
- Add key bindings for scaling (`+`, `-`) and orientation (`o`)
- Maintain backward compatibility with existing patches

**Non-Goals:**
- Persist scale/orientation state across sessions
- Hardware bridge integration (MIDI, network)
- Adding new component kinds or modifying existing ones
- Changing the .ini patch format
- Adding new controller types

## Decisions

### 1. Scale factor stored in App state
- **Decision**: Add `scale_factor: f32` and `orientation: Orientation` fields to `App` struct
- **Rationale**: Central state means renderer reads from single source; no need for per-component state
- **Alternative**: Store scale per-component - rejected by YAGNI, adds complexity without benefit

### 2. Orientation enum with two variants
- **Decision**: Define `enum Orientation { Portrait, Landscape }` and use in App
- **Rationale**: Simple two-state system matches the feature scope; panel reflow logic is straightforward
- **Alternative**: Continuous rotation angle - over-specified for the use case

### 3. Scale presets as discrete values
- **Decision**: Use fixed presets (0.5, 1.0, 1.5, 2.0) with `+`/`-` keys to cycle
- **Rationale**: Predictable, testable, avoids floating-point drift; matches common UI scaling patterns
- **Alternative**: Free-text scale input - rejected as outside scope, adds complexity

### 4. Renderer computes transformed geometry
- **Decision**: Renderer multiplies component rects by scale factor and reflows based on orientation
- **Rationale**: Renderer already owns layout and publishes component_rects; transformation logic lives in one place
- **Alternative**: Handler computes geometry - violates separation of concerns; renderer knows where components land

### 5. Orientation switching reflows components
- **Decision**: Portrait = vertical panels; Landscape = horizontal rows
- **Rationale**: Matches the existing panel grouping by controller; minimal code change
- **Alternative**: Rotate entire layout 90 degrees - would require major renderer rewrite

### 6. Minimum component size enforcement
- **Decision**: Enforce 40px width / 20px height minimum regardless of scale
- **Rationale**: Prevents unreadable component collapse at 50% scaling; matches design guidelines
- **Alternative**: No minimum - risk of unusable UI at low scales

## Risks / Trade-offs

- **[Risk]** Scale factor interactions with shift groups: scaling could affect how shift-group borders are rendered
  - **Mitigation**: Test all shift group combinations at each scale preset
- **[Risk]** Orientation reflow with mouse hit-testing: component_rects must be recalculated on orientation change
  - **Mitigation**: Orientation change triggers fresh render cycle; hit-testing uses new rects
- **[Risk]** Keyboard + mouse agreement at different scales: hit-test areas change with scale
  - **Mitigation**: Verify mouse coordinates scale proportionally with component rects
- **[Trade-off]** Discrete scale presets vs. continuous scaling: presets are simpler but less flexible
  - **Decision**: Presets chosen for simplicity and testability; can add continuous later if needed

## Open Questions

- How should the picker overlay render at different scales? (Defer - can observe behavior in testing)
- Should scaling affect component hit-test sensitivity proportionally? (Yes - component_rects scale with factor)