# Labels

`droid_tui` never edits your `.ini` files. Names you assign live in a
label store and overlay the patch text everywhere: Module UI cells,
graph node titles, and source-viewer headers.

## Editing

Focus a Module UI cell, a source header instance, or hover a graph
node, then press `e`. Type the label, `Enter` saves, `Esc` cancels.
Inside the overlay, `1`–`N` (N = your `max_shift_layer`, see
[Configuration](config.md#labels)) switches which shift-group layer
you are editing, keeping a per-layer draft.

## Resolution order

A hardware label resolves through four layers, first hit wins:

1. The store entry for the active shift layer.
2. The store entry for layer 1.
3. A `# TOKEN: label` preamble comment in the patch file.
4. The derived default (the token id itself).

So a `# B1.1: [KICK]` comment at the top of your patch names `B1.1`
without any store entry, and a stored layer-1 label beats it.

## Shift layers

With `[labels] layers_enabled` (default on), each `B*`/`P*`/`S*`/
`E*`/`I*`/`G*`/`O*` token can carry one label per shift group
(1 to `max_shift_layer`, default 4, clamped 1–8). Pressing `1`–`4`
on the Module UI switches the displayed layer. With layers disabled,
everything displays layer 1 while layers 2–N stay preserved in the
store.

## Circuit labels

A per-circuit override (keyed by circuit name + instance) replaces the
source header and graph node title in both the full and filtered
graph panes. Edited through the same `e` overlay on a graph node.

## Guessed labels

Circuits with no stored label inherit the nearest explicit upstream
label (found by walking producer edges back to a circuit that has
one). Titles resolve store → guess → raw token. Guesses refresh on
every graph build and label save.

## Storage

Labels live at `$XDG_CONFIG_HOME/droid-tui/labels.toml`
(`~/.config/droid-tui/labels.toml` by default), keyed by the
canonicalized absolute patch path — moving the patch starts a fresh
bucket. Writes are atomic; a corrupt file warns once and falls back
to empty.
