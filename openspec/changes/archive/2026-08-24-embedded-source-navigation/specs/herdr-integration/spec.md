# Herdr Integration Specification (delta)

## REMOVED Requirements

### Requirement: Herdr pane launch
The system SHALL NOT spawn a herdr pane (`herdr pane split` / `herdr pane run`) to host the source viewer. The viewer is embedded in the main TUI process and cannot receive selection or highlight state across a process boundary.

**Reason**: The embedded source pane renders inside the running TUI, making external pane launching dead code; keeping it would violate one-source-of-truth for "where is the viewer".
**Migration**: Press `g` then `v`; the source opens as a split pane inside the same window.

### Requirement: Fallback terminal window launch
The system SHALL NOT detect `$TERM` or spawn kitty/xterm/gnome-terminal/alacritty windows for the source viewer.

**Reason**: Same as above — no second process exists, so terminal-emulator fallback launching has no consumer.
**Migration**: Press `g` then `v`; no external window is opened.

### Requirement: Viewer mode tracking
The system SHALL NOT track how an external viewer was launched (`ViewerMode::None/Herdr/Fallback`); there is nothing left to record.

**Reason**: The mode enum existed solely to report external-launch outcomes; with embedding, launch failure handling disappears entirely.
**Migration**: No user action required; launch-failure status messages no longer occur.
