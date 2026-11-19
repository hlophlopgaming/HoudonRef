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

Stage 1a is specified in issue #1 and split into tickets #2 to #19 and #23 to #25.

Done:

- [x] Scene with several objects, Figures at real height, metric or imperial units (#2, #3, #4).
- [x] Up to 16 directional, point and spot lights (#5), with shadows (#12).
- [x] Room: floor sweeping into the backdrop, built-in grey, white and dark studios (#6).
- [x] glTF materials; colour from materials, Clay or Random (#7).
- [x] Built-in reference heads: Asaro head and planar head.
- [x] Value mode: 2 to 5 tones, grey and squint (#25).
- [x] Anti-aliasing: 4× MSAA in the viewport, supersampled offscreen pictures (#23).

Next:

- [ ] Light masks (#13), Solid mode with matcaps (#15), cavity (#16), contour lines and wireframe (#17).
- [ ] Smooth shadow edges and per-light shadow softness (#24).
- [ ] Scene files (#11), presets for light, room and camera (#18).
- [ ] OBJ import (#14); desktop `.blend` import through an installed Blender (#8); web and Android show conversion instructions instead.
- [ ] Ambient occlusion (#9), PNG export and presentation mode (#10), automatic quality (#19).

Stage 1b: area lights with soft shadows, the Render mode (ADR 0001), HDRI rooms, dragging lights in the viewport.

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

## After release: ideas

Not tickets yet; to be grilled and specified when their time comes.

### Drawing aids (proposed for before release where cheap)

- **Compare with my drawing:** load a photo of the artist's drawing and lay it over the picture with transparency or as a difference, to see where proportions and angles drift.
- **Grids and guides over the picture:** rule of thirds, proportional grid, plumb lines, horizon and vanishing points; measuring angles and ratios on screen, like sighting with a pencil.
- **Mirror the picture** with one key, to see one's own errors afresh.

### Studio and Atelier

- **Basic forms:** sphere, cube, cylinder, cone and box as built-in models for exercises.
- **Light as in life:** colour temperature in kelvins; the sun by time of day.
- **Second screen:** open the same scene on a tablet or phone by a link or QR code and draw from it.
- **Layers for painting:** export silhouette, shadows, light and contour lines as separate layers (ties in with the Krita bridge below).
- **Study model library:** skulls, hands, feet, plaster casts and drapery; part of it comes with the Atlas.

### Blender and Krita bridge (desktop)

- **Live link with Blender.** A Blender add-on sends scene changes to the Studio over a local connection: move a light or an object in Blender and the Studio picture follows.
- **Scene import from Blender.** Bring a whole Blender scene into the Studio: objects, their materials, lights and the camera (glTF with `KHR_lights_punctual` covers lights; the camera maps to a Camera preset). Builds on the desktop `.blend` import (ADR 0002).
- **Krita docker.** A Krita plugin (Python) that shows the Studio picture in a docker next to the canvas and refreshes it live, so the reference sits inside the painting program. Possibly also sending render passes (lines, shadows, light) as Krita layers.
- Open questions: one-way or two-way sync; which Blender and Krita versions; whether the Krita side is worth it once the Studio window can stay on top.
