# mzera

A small, very fast photo & WebM viewer for Omarchy (Hyprland / Wayland), written in Rust with GTK4 + libadwaita.
Fully usable from the keyboard and fully usable from the mouse. Follows the Omarchy theme live.

Status: **planned and verified, no code yet.** Read `docs/explainer.html` in a browser, then `docs/PLAN.md`.
Evidence for every number and every design correction is in `docs/verification/`.

## What it is for

- open a folder recursively and just browse, first tiles in well under a second for 35 000 files
- gallery: flat grid, or grouped by folder with sticky headers and a jump list; thumbnail zoom
- format filters (only JPEG, hide WebP, …) with presets on 1–9; WebM video inline
- fullscreen viewer with instant next/prev, scale modes, zoom/pan, captions / metadata / clean, built-in key help
- launches from `Super+Alt+P`, the app launcher, Nautilus, or `mzera <dir>`

## Layout

```
docs/explainer.html        options, verification results, architecture, measured performance plan, input model, mockups
docs/PLAN.md               the plan in text form: phases and gates
docs/verification/         toolchain compile + cold start, thumbnail benchmark, GTK internals review, input model
docs/verification/skeleton verified Cargo.toml + main.rs (gtk4 0.11 / libadwaita 0.9 / glycin 3.1)
```

## Toolchain

Rust via rustup in user space: `export PATH="$HOME/.cargo/bin:$PATH"`.
