# Vendored iced_tiny_skia 0.14.1

Patched fork of the `iced_tiny_skia` crate (copied from the registry
source, `chmod u+w`'d), wired in through `[patch.crates-io]` in the
workspace `Cargo.toml`. Keep this directory under version control: an
earlier copy of these patches lived only in a scratch checkout and was
lost, which brought the ghosting bugs back.

## Patch 1 — full-window damage (`src/window/compositor.rs`)

On X11 + softbuffer, the `buffer.age()` of the back buffer does not
match the layer stacks recorded in `Surface`, so the incremental
`damage::diff` misses regions and stale pixels pile up as ghosting
around moving content (the growing composer, the scrolling editor).
`present()` now always rasterizes the whole window instead of trusting
the age-based diff.

## Patch 2 — editor clip mask (`src/engine.rs`)

The `Text::Editor` branch sized its `physical_bounds` from
`editor.bounds`, which is the widget's viewport size, never its
content. When the buffer scrolls to a half-line the last row spills
past the bottom edge while the viewport check keeps concluding "inside
bounds" and skips the clip mask. The Editor branch now always installs
the mask (editors are small; masking costs nothing), matching what the
Paragraph branch achieves with real `min_bounds`.

## Upstream status

iced 0.14.0 fixed related text damage (#2964) and buffer presentation
(#3032) issues, but both problems still reproduce on this stack
(GNOME/Mutter, softbuffer, `--software-rendering`). Re-check whether
the patches are still needed when bumping iced past 0.14.

Checked 2026-10-05 against upstream `master` @ e0c7739 (reference
clone at `~/workshop/rust/iced-upstream`, `git pull` it to refresh):

- Patch 1 (damage): **not fixed upstream**. `present()` on master
  still trusts `buffer.age()` + `damage::diff` (window/compositor.rs),
  so this patch must be kept regardless of release cadence.
- Patch 2 (editor clip): **fixed on upstream master** — the
  `Text::Editor` branch now unconditionally calls `adjust_clip_mask`,
  semantically equivalent to our patch. However no crates.io release
  contains it yet (`iced_tiny_skia` 0.14.1 still ships the buggy
  `editor.bounds` viewport shortcut). Drop Patch 2 (and eventually the
  whole vendor) once a released iced line includes the master fix.
- Yesterday's `capture_event` fix in `widget/src/text_editor.rs`
  (ee54913) is an event-layer change unrelated to these two render
  patches.
