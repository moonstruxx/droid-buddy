## 1. Setup

- [ ] 1.1 Add `ini` crate to Cargo.toml for .INI file parsing
- [ ] 1.2 Extend `ComponentKind` enum in `src/patch.rs` with `Encoder` variant
- [ ] 1.3 Extend `ComponentState` if needed for new component types
- [ ] 1.4 Verify existing code compiles with new crate added

## 2. Patch Parsing

- [ ] 2.1 Implement .ini file parser to extract circuit sections (`[button]`, `[p2b8]`, `[faderbank]`, etc.)
- [ ] 2.2 Add regex-based hardware token extraction from all section values (`B1.1`, `L1.2`, `P1.1`, `O1`, `I1`, `E1.1`, `S1.3`)
- [ ] 2.3 Map token prefixes to `ComponentKind`: `B`→Button, `L`→Led, `P`→Knob, `O`→CvOut, `I`→CvIn, `E`→Encoder, `S`→Switch
- [ ] 2.4 Derive patch name from `.ini` filename (without extension)
- [ ] 2.5 Associate tokens with shift groups based on circuit context (P2B8 buttons 1-8 share shift behavior)
- [ ] 2.6 Populate `Patch.hw_components` and `Patch.shift_groups` from parsed data

## 3. File Picker UI

- [ ] 3.1 Add file picker state to `App` struct (`showing_picker: bool`, `picker_dir: Path`, `selected_file: Option<Path>`)
- [ ] 3.2 Implement directory listing with `.ini` filtering (show `.ini` files normally, dim non-`.ini`)
- [ ] 3.3 Handle navigation: j/k or arrow keys to move, Enter to select directory/file, `..` for parent directory
- [ ] 3.4 Handle `Esc` to cancel and close picker
- [ ] 3.5 Trigger picker with `l` key when no patch is loaded
- [ ] 3.6 Load selected patch on Enter, parse with new parser, close picker
- [ ] 3.7 Handle `l` key when patch is loaded (reload with new file)

## 4. Controller Panel Layout Redesign

- [ ] 4.1 Rewrite `render_patch` in `src/ui.rs` to group components by controller type (P2B8, Faderbank, Notebuttons, Encoder, etc.)
- [ ] 4.2 Create panel rendering: bordered title with controller type name (e.g., " P2B8 ", " Faderbank ")
- [ ] 4.3 Arrange components within panels in physical order (B1.1→B1.8 left-to-right)
- [ ] 4.4 Render each component with label and state (●/○ for buttons, ◉% for knobs, →/← for CV)
- [ ] 4.5 Handle panel overflow: wrap components to multiple rows when terminal width is insufficient
- [ ] 4.6 Add resize event handling to reflow panels on terminal size change

## 5. Mouse Interaction Support

- [ ] 5.1 Enable mouse capture in `src/main.rs` on startup via `crossterm::event::EnableMouseCapture`
- [ ] 5.2 Disable mouse capture on exit (via `q`/Ctrl+C) for clean terminal restoration
- [ ] 5.3 Handle `Event::Mouse` in main event loop alongside `Event::Key`
- [ ] 5.4 Implement click-to-toggle: clicking a button/switch toggles its ON/OFF state
- [ ] 5.5 Implement hover highlight: component under mouse cursor gets reversed colors
- [ ] 5.6 Implement mouse wheel scrolling: scrolling over a knob/fader increments/decrements its value
- [ ] 5.6 Ensure Herdr/tmux mouse compatibility: verify mouse events pass through multiplexer

## 6. Shift Visualization

- [ ] 6.1 When shift key 1-4 is pressed, activate the corresponding `ShiftGroup`
- [ ] 6.2 Identify which controller panels contain components from the active shift group
- [ ] 6.3 Render affected panels with a colored bold border (Group1=Yellow, Group2=Cyan, Group3=Magenta, Group4=Green)
- [ ] 6.4 Render unrelated panels with dim gray border
- [ ] 6.5 Clear shift on `Esc` press, restore all panels to default borders
- [ ] 6.6 Show "SHIFT N ACTIVE" in status bar with group color and bold styling when active
- [ ] 6.7 Maintain shift state during mouse interaction (clicking components while shift is active)

## 7. Preserve Keyboard Navigation

- [ ] 7.1 Ensure all existing key bindings continue to work: j/k navigation, Enter/Space to toggle, 1-4 for shift, Esc to clear
- [ ] 7.2 Ensure keyboard and mouse interaction are mutually compatible (no interference)
- [ ] 7.3 Handle modifier key detection for Ctrl+C quit

## 8. Testing and Verification

- [ ] 8.1 Build project and verify no compilation errors
- [ ] 8.2 Test with sample patch: verify components render correctly
- [ ] 8.3 Test .ini file loading: verify token extraction and component display
- [ ] 8.4 Test file picker: navigate directories, select patch, cancel with Esc
- [ ] 8.5 Test mouse interaction: click toggle, hover highlight, scroll adjustment
- [ ] 8.6 Test shift keys 1-4: verify colored borders and status bar updates
- [ ] 8.7 Test terminal resize: verify panels reflow correctly
- [ ] 8.8 Verify Herdr/tmux compatibility: mouse and keyboard work in multiplexer environment