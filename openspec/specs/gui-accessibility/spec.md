# Gui Accessibility Specification

## Purpose

Make droid_tui's custom-painted GUI surfaces discoverable by assistive technologies and queryable by egui_kittest tests through AccessKit widget annotations.

## Requirements

### Requirement: Paint functions accept Ui reference

All top-level paint functions (paint_scene, paint_panels, paint_physical) SHALL accept `&mut Ui` instead of `&Painter`, accessing the painter via `ui.painter()`.

#### Scenario: paint_scene receives Ui

- **WHEN** paint_scene is called from the render loop
- **THEN** it receives `&mut ui` and accesses the painter via `ui.painter()`

#### Scenario: paint_panels receives Ui

- **WHEN** paint_panels is called from the render loop
- **THEN** it receives `&mut ui` and accesses the painter via `ui.painter()`

#### Scenario: paint_physical receives Ui

- **WHEN** paint_physical is called from the render loop
- **THEN** it receives `&mut ui` and accesses the painter via `ui.painter()`

### Requirement: Graph nodes emit AccessKit annotations

Each graph node rendered by paint_scene SHALL register an AccessKit node with `WidgetInfo::labeled(WidgetType::Unknown, enabled, label)` where label is the circuit name + instance.

#### Scenario: Graph node is queryable by label

- **WHEN** a graph is open with nodes
- **THEN** each node is queryable via `harness.get_by_label("CircuitName 1")`

### Requirement: Panel components emit AccessKit annotations

Each hardware component cell rendered by paint_panels SHALL register an AccessKit node with `WidgetInfo::labeled(WidgetType::Button, enabled, label)` where label is the component's display label.

#### Scenario: Panel component is queryable by label

- **WHEN** panels are rendered with hardware components
- **THEN** each component is queryable via `harness.get_by_label("B1.1")`

### Requirement: Physical cells emit AccessKit annotations

Each element cell rendered by paint_physical SHALL register an AccessKit node with `WidgetInfo::labeled(WidgetType::Button, enabled, label)` where label is the cell's label text.

#### Scenario: Physical cell is queryable by label

- **WHEN** the physical view is open
- **THEN** each cell is queryable via `harness.get_by_label("B1.1")`
