<p align="center"><img src="assets/logo/houdonref-sanguine.svg" width="160" alt="HoudonRef logo"></p>

<h1 align="center">HoudonRef</h1>

<p align="center">Open-source tool for artists: an anatomy atlas, a 3D reference studio and a timed drawing atelier.</p>

> Status: early development. The Studio can show a glTF model under adjustable light; Atlas and Atelier are not started. See the [roadmap](docs/ROADMAP.md).

Code licence: [GPL-3.0](LICENSE). Logo and third-party content keep their own licences (see Credits).

## What it is

HoudonRef helps artists learn form, light and anatomy by drawing from 3D models. It has three sections:

| Section | What it is for |
| --- | --- |
| **Atlas** | Interactive écorché by layers (skin → superficial muscles → deep muscles → skeleton), names in English, Russian and Latin, later animals |
| **Studio** | View and pose your own models (glTF, OBJ, FBX, .blend via Blender), full lighting setup with presets and rooms, still-life generator, reference-search plugins |
| **Atelier** | Timed drawing sessions: complexity progression (Loomis ball → planes → detailed head with the same light), rotating camera angles, pose changes, light changes, focal length and focus |

## Principles

- Runs on weak hardware: rendering targets the OpenGL ES 3.0 / WebGL2 feature level.
- One codebase for desktop (Windows, Linux, macOS), web and Android.
- Rust + `wgpu` + `egui`.
- Open data: series, light presets and rooms are plain files (glTF + RON).

## Building

Requires a recent stable Rust toolchain.

Desktop:

```bash
cargo run --release -p houdon-app
```

Web (needs [trunk](https://trunkrs.dev/) and the `wasm32-unknown-unknown` target):

```bash
cd crates/houdon-app && trunk serve --release
```

Tests:

```bash
cargo test --workspace
```

## Opening models

Use **Add → Open a model file…** or drop a `.glb`, `.gltf` or `.obj` file onto the
Studio. An OBJ's `.mtl` materials and textures are read from beside it on
the desktop; in the browser, pick or drop them together with the `.obj`.
An OBJ without them opens in Clay, with a note naming the missing files.

## Saving Studio scenes

In the Studio toolbar, use **Scene → Save scene** and **Scene → Open scene**,
or drop a `.ron` file onto the window to open it. Desktop saves normally reference the original model
paths. Enable **Embed models in scene file** to share the scene without
those files. Browser downloads always embed the models, OBJ materials and textures included. A missing model
is reported when opening a scene, and the remaining objects still load.

## Reusing Studio lights

Under **Lights → Light presets**, choose a preset and select **Apply**.
Name the current lights and select **Save current lights** to reuse them
in other scenes. **Export…** writes a portable `.light.ron` file with its
masks. Open it with **Import…** or drop it onto the Studio.

## Camera and room presets

Under **Camera**, the buttons Front, Three-quarter, Profile, From above
and From below each set a direction, height and lens, and frame the whole
scene or the selected object, so a figure and a skull are framed alike.
**My camera presets** saves the current view relative to what it frames;
**Room → My rooms** saves the floor, backdrop and ambient light. Both
export to `.camera.ron` and `.room.ron` files, which **Import…** or a
drop onto the Studio brings back.

## Credits

- Name after Jean-Antoine Houdon, sculptor of the *Écorché* (1767).
- Logo after *A kneeling écorché figure* by J. Gamelin (1778/79). Source image: [Wellcome Collection](https://wellcomecollection.org/works/gv249kuf), CC BY 4.0.
- Planned anatomy data: [Écorché](https://github.com/Charleeh/ecorche) and [Z-Anatomy](https://www.z-anatomy.com/), both CC BY-SA 4.0. Anatomy content will ship as separate content packs under its own licence.
- Test models: *Planar Human Base Rigs* by dacancino, after a rig by Alexander Lee, CC BY 4.0. See [assets/models/test/CREDITS.md](assets/models/test/CREDITS.md).
- Studio matcaps: Blender community, CC0 or public domain. See [assets/matcaps/CREDITS.md](assets/matcaps/CREDITS.md).
- Reference heads: *Asaro-head* by Ivan Chakir, *Planar head (Oleg Toropygin)* by BlueHorse, *Zbrush head base* by hexmeff and *Wolfsaro Head - Anthro Canine Head Reference* by mmdrawstuff, all CC BY 4.0. See [assets/models/heads/CREDITS.md](assets/models/heads/CREDITS.md).
- Light presets: Three-point, Sunrise and Cloudy are adapted from the Bforartists Default Library. See [assets/presets/CREDITS.md](assets/presets/CREDITS.md).
- Light masks: *Light Masks* by Kenney (kenney.nl), CC0. See [assets/masks/CREDITS.md](assets/masks/CREDITS.md).
