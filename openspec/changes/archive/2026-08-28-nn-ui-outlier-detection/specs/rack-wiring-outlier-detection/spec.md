# rack-wiring-outlier-detection Delta

## MODIFIED Requirements

### Requirement: Wiring-outlier topology warning
The system SHALL flag a binding as a wiring outlier when a compact learned decision artifact, fitted offline from a rebalanced labeled corpus and embedded in the binary, classifies the binding as implausible — and it is wired directly (zero cable hops) — producing a topology warning that the signal-flow graph renders with the error-highlight token. Bindings that reach a distant target via a cable, or that are physically adjacent or co-located, SHALL NOT be flagged regardless of the artifact's output. The artifact SHALL be a static, deterministic, in-process scorer with no external model runtime.

#### Scenario: Outlier flagged
- **WHEN** a binding connects a far-left modifier encoder directly to a far-right fader without a cable
- **THEN** the graph renders the offending edge with the error-highlight token and reports a topology warning

#### Scenario: Via-cable not flagged
- **WHEN** a binding reaches a far target through a cable
- **THEN** the binding is not flagged, even if the artifact would otherwise classify it as implausible

#### Scenario: Adjacent not flagged
- **WHEN** a binding connects adjacent buttons
- **THEN** the binding is not flagged, even if the artifact would otherwise classify it as implausible

#### Scenario: Co-located LED and button never flagged
- **WHEN** a binding connects a co-located `L→B` pair
- **THEN** the binding is not flagged, even if the artifact would otherwise classify it as implausible

#### Scenario: Artifact replaced without behavior change
- **WHEN** the embedded artifact is refitted from a new corpus snapshot
- **THEN** no change to the warning channel, the render path, or the topology validation flow is required

### Requirement: Per-token influence second opinion
The system SHALL surface a topology warning for a hardware token whose `influence_subtree` size deviates from the corpus distribution for its token kind by more than a calibrated threshold (a z-score), using corpus mean and standard deviation baked into the binary. The second opinion SHALL travel through the same `TopologyIssue` warning channel and error-highlight token as the wiring-outlier finding, and SHALL NOT gate patch loading.

#### Scenario: Statistically unusual influence flagged
- **WHEN** a patch contains a hardware token whose influence subtree is an extreme outlier for its token kind
- **THEN** the graph renders the associated cable with the error-highlight token and reports a topology warning

#### Scenario: Typical influence not flagged
- **WHEN** every hardware token's influence subtree size is within the calibrated z-score band
- **THEN** no additional topology warning is produced

#### Scenario: Warning does not block loading
- **WHEN** a per-token z-score warning is present
- **THEN** the patch still loads and the warning appears only in the topology findings