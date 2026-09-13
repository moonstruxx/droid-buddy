## REMOVED Requirements

### Requirement: Narrow-terminal fallback
**Reason**: There is no terminal width; panes are pixel-sized and reflow with the window.
**Migration**: Drop the sub-120-column collapse and the "+N views hidden" status hint.
