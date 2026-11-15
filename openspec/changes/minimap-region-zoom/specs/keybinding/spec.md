# Spec Delta

## ADDED Requirements

### Requirement: Minimap drag gesture

The graph pane SHALL treat a drag inside the minimap as a region-zoom gesture: press anchors, drag tracks a marquee, release fits the camera to that region, and a drag shorter than three pixels on either axis stays a click-to-pan. `Esc` SHALL cancel an active marquee without closing the graph. The help modal's graph table and the user manual SHALL document the gesture.

#### Scenario: Help documents the gesture

- **WHEN** the user opens the graph help view
- **THEN** the key table lists the minimap drag as "zoom to dragged region"

#### Scenario: Escape cancels without closing

- **WHEN** the user presses `Esc` while a minimap marquee is active
- **THEN** the marquee is discarded and the graph pane stays open
