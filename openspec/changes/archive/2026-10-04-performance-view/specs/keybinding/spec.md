### Requirement: Performance-view keybindings (MODIFIED)

The Physical pane SHALL bind `p` (no modifiers) to opening the performance
view and provide a reset key restoring element states; `Esc` SHALL leave the
performance view. All other panes keep their existing `p` meaning.

#### Scenario: Help documents the keys

- WHEN the user opens the module-UI help view
- THEN the key table lists `p` with its performance-view meaning and the
  reset key with its restore meaning.
