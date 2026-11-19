# Roadmap

HoudonRef has three sections (Atlas, Studio, Atelier) on one renderer. The
order below builds the shared pieces first and leaves the content-heavy
work for later.

## Architecture

One Cargo workspace:

| Crate | Responsibility |
| --- | --- |
| `houdon-core` | Model data and loading, bounds, camera math. No GPU, no UI; unit-tested. |
| `houdon-render` | 3D viewport on `wgpu`, limited to the WebGL2 / OpenGL ES 3.0 feature level (no compute, no storage buffers). |
| `houdon-i18n` | Interface translations with Fluent. English source strings, other languages in `locales/<code>/main.ftl`. |
| `houdon-app` | `eframe` / `egui` shell with the three sections; desktop and web entry points. |

Later: `houdon-import` for formats beyond glTF, and a content-pack crate for the Atlas.

## Stage 0: foundation (in progress)

- [x] Workspace, `rustfmt`, `clippy`, tests.
- [x] App shell with Atlas / Studio / Atelier tabs, English and Russian, language remembered between runs.
- [x] glTF / GLB viewer with an orbit camera (orbit, pan, dolly, focal length in mm).
- [x] Models normalised to a real height, so lens and distance mean the same thing for every model.
- [x] Drag and drop of `.glb` / `.gltf` files on desktop and web; a clear message for `.blend`.
- [x] CI: lint and tests, Windows and macOS builds, web build with trunk, deploy to GitHub Pages from `main`.
- [ ] Android build in CI.

## Stage 1: Studio MVP

- Key, fill and rim lights; shadow maps; light presets as RON files.
- Backgrounds and rooms.
- Saved camera views.
- OBJ import.
- Desktop `.blend` import through an installed Blender (headless export to glTF). Web and Android show conversion instructions instead.

## Stage 2: Atelier MVP

- Timed sessions described by a series manifest (RON): steps, durations, camera angles, light.
- Procedural primitives for the complexity progression: Loomis ball → planes → detailed head.
- Camera angle and light change between rounds.

## Stage 3: Atlas

- Content pipeline: Z-Anatomy / Écorché → light glTF files by layer, shipped as a separate CC BY-SA content pack.
- Content packs are downloaded on demand (GitHub Releases or a separate repository), not bundled in the web build.
- Layer toggles, picking a muscle or bone, names in English, Russian and Latin.

## Stage 4: extensions

- Posing. The approach is not chosen yet: our own rigs or the skeletons of imported models.
- FBX import.
- Still-life generator.
- Reference-search plugins.
- Depth of field.
- Animals.
- Android polish.

## Decisions

- Web version: GitHub Pages, static only. User models are opened from the local disk in the browser.
- `.blend`: no native parser. The format changes between Blender versions and modifiers and rigs need Blender itself to evaluate.
