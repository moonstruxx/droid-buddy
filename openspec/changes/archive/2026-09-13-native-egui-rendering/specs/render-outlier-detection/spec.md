## REMOVED Requirements

### Requirement: Render-metrics extraction
**Reason**: Terminal-width degradation no longer exists; rendering is in native pixels.
**Migration**: Delete the render-metrics extractor and its tests.

### Requirement: Embedded distilled scorer
**Reason**: The scorer detects terminal-width degradation, which no longer applies.
**Migration**: Delete the render-outlier scorer and the embedded table.

### Requirement: Invariant guards
**Reason**: There is no terminal-width degradation to guard against.
**Migration**: Delete.

### Requirement: Status-hint surface
**Reason**: The "Renders degraded at N cols" hint is terminal-specific.
**Migration**: Delete the hint and the `render_outlier_warning` theme token.

### Requirement: Gallery-CI render-outlier flag
**Reason**: The gallery that carried these flags is replaced by egui assertions.
**Migration**: Delete.
