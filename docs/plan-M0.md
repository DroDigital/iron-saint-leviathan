# M0 — Skeleton

**Scope:** workspace, window, wgpu clear, input actions, fixed timestep, CI on three OSes.

**Risks:** wgpu API drift (v30); CI runners without GPUs.

**Test plan:** cargo test; CI matrix green.

**Status:** delivered (see CHANGELOG.md). Deviations and findings are recorded in docs/DESIGN_NOTES.md.
