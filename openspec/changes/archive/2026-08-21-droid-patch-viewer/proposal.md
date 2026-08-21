# OpenSpec Proposal: droid-patch-viewer

## Why

The `droid_tui` application currently supports loading, inspecting, and interacting with DROID `.ini` patches, but lacks a dedicated source code viewer for examining patch structure. This feature adds a keycode-activated viewer that displays patch circuits in a prettified, diff-viewer-style layout with sidebar jump points, optimized for fast overview and navigation.

The design was refined through exploration mode with user feedback confirming:
- Prefix key `g` + `v` to open viewer
- Diff-viewer style layout (sidebar + main area)
- Herdr integration with fallback second window
- Readonly, scrollable, ESC to close

## What Changes

### New capability: Source code viewer for DROID patches

1. **Keybinding**: `g` prefix → `v` opens the source viewer (extensible for future `g`-commands)
   - Avoids conflicts with existing keys (`q`=quit, `l`=picker, `1-4`=shift, `Esc`=clear, `Enter/Space`=toggle, `j/k/arrows`=navigate)
   - Vim-style modal pattern for extensibility

2. **Viewer layout** (two-pane, diff-viewer style):
   - **Left sidebar**: all circuits as jump points (like a diff viewer's file list)
   - **Main area**: prettified circuit blocks (chat-bubble style), optimized for readability and fast overview
   - **Navigation**: select circuit in sidebar → main area jumps to it; or scroll the main area directly
   - **ESC**: closes the viewer, returns focus to main TUI

3. **Herdr + Fallback integration**:
   - `HERDR_ENV=1` → `herdr pane split` → viewer runs in the new pane
   - Not in herdr → open a second window (`kitty @ new-window` launching viewer)
   - Viewer is readonly: no component toggling, no state mutation

4. **Viewer behavior**:
   - Parses the loaded `.ini` patch
   - Renders each section as a bordered chat-bubble block
   - Uses existing component-kind colors (buttons=white, knobs=magenta, cv-in=cyan, cv-out=green, led=red)
   - Sidebar lists all circuit names for quick jumping
   - Terminal-native scroll (j/k/arrows, PgUp/PgDn, mouse wheel)
   - No zoom (removed per feedback)

## Capabilities

### New capabilities enabled:
- Keybinding `g` prefix for extensible command mode
- Source code viewer with circuit visualization
- Herdr pane integration OR secondary window fallback
- Diff-viewer style two-pane layout

### Modified capabilities: None
(Pure addition, no existing behavior changed)

## Impact

### Files affected:
- `src/ui.rs`: Add viewer render function, prefix key handling, new key dispatch
- `src/handler.rs`: Add `g` prefix key handling, `v` to trigger viewer, ESC to close
- `src/app.rs`: Add viewer state (`showing_viewer`, `viewer_patch`, `viewer_selected_circuit`)
- `src/main.rs`: Dispatch `g` key events to viewer handler
- `openspec/changes/droid-patch-viewer/`: Generated artifacts (proposal.md, specs/, tasks.md)

### Test impact: 24 existing tests unchanged
(Viewer is new feature, not modifying existing behavior)

### Documentation impact:
- `ARCHITECTURE.md`: May add section on viewer integration
- `DESIGN.md`: Document the viewer layout and keybinding pattern