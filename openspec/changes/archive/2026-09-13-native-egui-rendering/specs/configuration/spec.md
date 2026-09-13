## REMOVED Requirements

### Requirement: GUI window configuration
**Reason**: The `[gui] graph_window` toggle is obsolete; the window is now the whole application and no longer optional.
**Migration**: Remove the `[gui]` section from config handling. Unknown keys are already ignored by the config loader.

### Requirement: GUI window flag seeded at startup
**Reason**: The runtime graph-window preference no longer exists.
**Migration**: Remove the graph-window seed from startup.
