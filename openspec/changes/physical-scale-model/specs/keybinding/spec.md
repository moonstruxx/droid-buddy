# Keybinding Specification

## Purpose

Define keyboard shortcuts for the TUI. The physical scale model adds pan/zoom and skeleton-toggle keys on the main view.

## ADDED Requirements

### Requirement: Physical-view navigation keys

The system SHALL provide keys to pan the physical view when the rack overflows the terminal, to change zoom, and to toggle the skeleton reference mode.

#### Scenario: Pan keys move the viewport

- **WHEN** the rack overflows the terminal and the user presses a pan key
- **THEN** the viewport offset moves in the corresponding direction without changing zoom.

#### Scenario: Skeleton toggle switches presentation

- **WHEN** the user presses the skeleton-toggle key
- **THEN** the main view switches between full and skeleton presentation of the same layout, and back.