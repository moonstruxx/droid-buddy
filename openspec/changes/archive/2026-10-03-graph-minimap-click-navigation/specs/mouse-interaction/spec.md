# Spec Delta

## ADDED Requirements

### Requirement: Graph minimap click navigation

While the graph pane is open, clicking within the graph minimap SHALL pan the camera to the document position under the click, updating the viewport indicator.

#### Scenario: Click graph minimap pans camera
- **WHEN** graph pane is open and minimap is visible, AND user left-clicks inside the minimap panel
- **THEN** graph camera pans so the clicked minimap position maps to the world center of the visible viewport