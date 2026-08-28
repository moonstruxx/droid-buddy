# patch-writing Specification

## ADDED Requirements

### Requirement: Lossless .ini writing

The application MUST be able to write the loaded `Patch` back to a `.ini` file at a caller-supplied path, preserving the source file byte-for-byte when the original section order is written.

- The writer MUST slice the patch's `raw_lines` into section blocks by header line, so that comments and banners preceding a header travel with that section, and the preamble (lines before the first header) stays first.
- The writer MUST be deterministic: same patch + same order → identical bytes.
- The write MUST be atomic (temp file + rename).
- The writer MUST refuse to write to the source patch's canonicalized path (save-as only).

#### Scenario: Round-trip identity

Given a parsed patch, writing it with the original section order produces a file byte-identical to the source.

#### Scenario: Comments and banners travel with their section

Given a patch whose sections are preceded by comments and `# ---- Name ----` banners, writing any reordering keeps each comment/banner attached to its own section in the output.

### Requirement: Section-reorderable output

The application MUST accept an arbitrary permutation of section indices and write a valid, re-parseable `.ini` file whose sections appear in that order.

- The output MUST re-parse to a `Patch` with the same sections, components, cables, and spans (order aside).
- If the destination path already exists, the writer MUST NOT overwrite it — it auto-suffixes (`-latopt.ini` → `-latopt-1.ini`, …) or the caller supplies a distinct path.

#### Scenario: Reordered output re-parses

Given a candidate ordering, writing it and re-parsing yields the same set of sections in the candidate order.

#### Scenario: Destination collision

Given an existing destination file, the write targets the next free suffixed name instead of overwriting.