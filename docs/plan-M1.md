# M1 — PSX renderer

**Scope:** low-res target, vertex snap, affine mapping, dithering, fog, Gouraud, glTF import, texture quantizer, test scene.

**Risks:** near-plane clipping with snapping; 15-bit everywhere.

**Test plan:** headless lavapipe render test asserting 15-bit output; screenshot scenarios.

**Status:** delivered (see CHANGELOG.md). Deviations and findings are recorded in docs/DESIGN_NOTES.md.
