# HoudonRef user interface, English (source language).

app-title = HoudonRef
section-atlas = Atlas
section-studio = Studio
section-atelier = Atelier
language = Language

atlas-loading = Loading the anatomy…
atlas-load-failed = The anatomy could not be loaded: { $reason }
atlas-layers = Layers
atlas-layer-bones = Bones
atlas-layer-muscles = Muscles
atlas-layer-fasciae = Fasciae and bursae
atlas-layer-joints = Ligaments and joints
atlas-opacity = See-through
atlas-look = Look
atlas-color-natural = Natural
atlas-frame = Frame the body
atlas-credit = Anatomy: Z-Anatomy (CC BY-SA 4.0), built on BodyParts3D (CC BY 4.0).
atelier-coming = The drawing atelier is coming: timed sessions that go from a Loomis ball to planes to a detailed head, with changing angles and light.

model-stats = { $triangles } triangles

studio-objects = Objects
studio-object-duplicate = Duplicate
studio-object-delete = Delete
studio-frame-scene = Frame scene
studio-frame-selected = Frame selected
studio-selected-object = Selected object
studio-no-selection = Click an object in the view or in the list to select it.
studio-position = Position
studio-rotation = Rotation
studio-size = Size
studio-camera-hint =
    Drag: orbit around the scene
    Right-drag or Shift + drag: pan
    Wheel or pinch: move closer or farther
    Click: select · double-click: frame it
    Delete: remove the selected object
    V: value study on and off
    P: presentation, only the view (P or Esc to leave)
studio-drop-hint = Drop a .glb, .gltf or .obj file onto the window to add it (an .obj together with its .mtl and textures).

studio-camera = Camera
studio-focal-length = Focal length

studio-light-azimuth = Direction
studio-light-elevation = Height
studio-light-intensity = Intensity

error-blend-file = .blend files cannot be opened directly yet. In Blender, use File → Export → glTF 2.0 (.glb) and open that file.
error-unsupported-file = Unsupported file type: { $name }. Drop a model (.glb, .gltf or .obj), an image, or a scene (.ron).
obj-files-missing = Opened { $name } without { $files }. Parts without their material are shown in Clay. Open the .obj together with its .mtl and texture files to see its materials.
error-load-failed = Could not open { $name }: { $reason }
error-read-failed = Could not read { $name }.
scene-open = Open scene
scene-save = Save scene
scene-embed-models = Embed models in scene file
scene-file-type = HoudonRef scene
scene-open-error = Could not open scene { $name }: { $reason }
scene-save-error = Could not save the scene: { $reason }
scene-model-skipped = Model { $name } could not be loaded. Its objects were skipped.

studio-object-label = { $model } { $number }
unit-metre-short = m
unit-millimetre-short = mm
model-male-figure = male figure
model-female-figure = female figure

settings = Settings
settings-units = Units
units-metric = Metric (cm, m)
units-imperial = Imperial (in, ft)
settings-figure-height = Figure height
settings-anti-aliasing = Smooth edges (anti-aliasing)
settings-anti-aliasing-hint = Turn off if the viewport is slow on a weak computer or phone.
settings-quality = Rendering quality
settings-quality-auto = Automatic (now: { $level })
settings-quality-low = Low
settings-quality-medium = Medium
settings-quality-high = High
settings-quality-hint = Sharper shadows and smoother ambient occlusion cost speed. Automatic picks by the graphics card and lowers it when the viewport stays slow.
studio-figure = Human figure (shown at figure height)
studio-dimensions = Width { $width }, height { $height }, depth { $depth }
studio-real-size = Real size (largest side)
studio-real-size-warning = The size in this file looks wrong (smaller than { $smallest } or larger than { $largest }). Enter the real size.
unit-centimetre-short = cm
unit-inch-short = in
unit-foot-short = ft
length = { $value } { $unit }

studio-lights = Lights
studio-lights-full = A scene holds up to { $count } lights.
studio-light-label = { $number }. { $kind }
studio-light-remove = Remove
studio-light-distance = Distance
studio-light-cone = Cone
studio-light-softness = Edge softness
light-directional = Directional
light-point = Point
light-spot = Spot

studio-room = Room
studio-room-visible = Show floor and backdrop
studio-room-floor = Floor
studio-room-backdrop = Backdrop
room-grey = Grey studio
room-white = White studio
room-dark = Dark studio
studio-ambient = Ambient light
studio-camera-direction = Direction
studio-camera-height = Height
studio-camera-distance = Distance
studio-light-casts-shadows = Casts shadows
studio-light-shadow-softness = Shadow softness
studio-light-shadow-softness-hint = How big the light looks: 0 is a bare bulb or the sun with crisp shadows, 1 a large softbox. Shadows blur more the farther they fall from what casts them.
studio-light-mask = Mask
studio-light-mask-none = None
studio-light-mask-remove = Remove mask
studio-light-mask-pattern = Pattern size
mask-picker = Light masks
mask-picker-hint = Click a mask to put it on the selected light. Drop a PNG or JPG on the window to use your own.
mask-credit = Light masks by Kenney (kenney.nl), CC0.
mask-unavailable = Unavailable
mask-category-windows = Windows
mask-category-foliage = Foliage
mask-category-caustics = Water
mask-category-cones = Cones
mask-category-streaks = Streaks
mask-category-circles = Circles
mask-category-fans = Fans
mask-category-shapes = Shapes
error-mask-needs-light = To use an image as a light mask, select a spot or directional light first, then drop the image.
error-mask-unreadable = { $name } is not a PNG or JPEG image that can be read.

studio-shading = Shading
shading-solid = Solid
shading-lit = Lit
shading-render = Render
shading-render-later = Coming in a later version: a higher-quality render on desktop.
studio-cavity = Cavity
cavity-hint = Brightens ridges and darkens hollows, so small changes of form read. In Solid and Lit.
cavity-geometry = From the mesh
cavity-screen = From the screen
cavity-ridge = Ridges
cavity-hollow = Hollows
studio-contours = Contour lines
contours-hint = Silhouettes and the inner folds of a form, as lines.
contours-thickness = Thickness
contours-folds = Folds
studio-wireframe = Wireframe
wireframe-opacity = Opacity
studio-backface-culling = Hide back faces
backface-culling-hint = Draw surfaces only from the side they face. Imported models are often open: then their inside disappears.
solid-studio = Studio
solid-matcap = Matcap
solid-flat = Flat
studio-matcap = Matcap
studio-matcap-none = None (choose one)
matcap-picker = Matcaps
matcap-picker-hint = Click a matcap to use it. While this window is open, drop an EXR, PNG or JPG on the window to use your own.
matcap-credit = Matcaps from the Blender community (CC0 or public domain).
error-matcap-unreadable = { $name } is not an EXR, PNG or JPEG image that can be read.

studio-color-source = Colour
color-material = Material
color-clay = Clay
color-random = Random
studio-clay-tone = Clay tone

studio-values = Values
values-off = Off
values-grey = Grey
values-tones = { $count } tones
values-balance = Light / shadow balance
values-squint = Squint

model-asaro-head = Asaro head
model-planar-head = planar head
model-base-head = base head
model-wolfsaro-head = Wolfsaro head

light-presets = Light presets
light-preset-choose = Choose a preset
light-preset-apply = Apply
light-preset-import = Import…
light-preset-export = Export…
light-preset-name = Preset name
light-preset-save-current = Save current lights
light-preset-file-type = HoudonRef light preset
light-preset-builtin-three-point = Three-point
light-preset-builtin-rembrandt = Rembrandt
light-preset-builtin-rim = Rim light
light-preset-builtin-top = Top light
light-preset-builtin-sunrise = Sunrise
light-preset-builtin-cloudy = Cloudy sky
light-preset-applied = Light preset applied.
light-preset-saved = Light preset saved.
light-preset-imported = Light preset imported.
light-preset-duplicate = A saved preset already has this name.
light-preset-load-error = Could not read the saved light presets: { $reason }
light-preset-save-error = Could not save the light preset: { $reason }
light-preset-import-error = Could not import the light preset: { $reason }
light-preset-export-error = Could not export the light preset: { $reason }

toolbar-scene = Scene
toolbar-add = Add
toolbar-open-model = Open a model file…
toolbar-values = Values
toolbar-frame-scene-hint = Fit the whole scene in view (double-click on empty space)
toolbar-frame-selected-hint = Fit the selected object in view (double-click it)
toolbar-help = Mouse and keyboard
menu-figures = Figures
menu-heads = Heads
menu-male-figure = Male figure (test)
menu-female-figure = Female figure (test)
menu-asaro-head = Asaro head
menu-planar-head = Planar head
menu-base-head = Base head
menu-wolfsaro-head = Wolfsaro head (canine)
model-file-type = 3D models
image-file-type = Images
studio-display = Display
studio-no-objects = No objects yet: use Add in the toolbar.
studio-empty-viewport = Add a model: the Add menu above, or drop a .glb, .gltf or .obj file here.
studio-add-light-menu = + Add light
shading-solid-hint = A fixed light for reading form; the scene's lights are ignored.
shading-lit-hint = The scene's lights, shadows and room.
values-key-hint = V over the viewport turns the value study on and off.
mask-open-file = Open my own image…
matcap-open-file = Open my own matcap…

camera-preset-front = Front
camera-preset-three-quarter = Three-quarter
camera-preset-profile = Profile
camera-preset-from-above = From above
camera-preset-from-below = From below
camera-preset-lens = { $mm } mm lens
camera-target = Frame:
camera-target-scene = whole scene
camera-target-selected = selected object
camera-presets-mine = My camera presets
room-presets-mine = My rooms
preset-none-saved = Nothing saved yet.
preset-choose = Choose a preset
preset-apply = Apply
preset-export = Export…
preset-import = Import a preset file…
preset-name = Name
preset-save-current = Save current
preset-file-type = HoudonRef preset
preset-saved = Preset saved.
preset-imported = Preset imported.
preset-duplicate = A saved preset already has this name.
preset-load-error = Could not read the saved presets: { $reason }
preset-save-error = Could not save the preset: { $reason }
preset-import-error = Could not import the preset: { $reason }
preset-export-error = Could not export the preset: { $reason }

studio-ambient-occlusion = Ambient occlusion
ambient-occlusion-hint = Darkens corners, contacts, folds and hollows, where the room's light reaches less. Lit mode only.
ambient-occlusion-strength = Strength
ambient-occlusion-radius = Radius
ambient-occlusion-samples = Quality
ambient-occlusion-samples-hint = Samples per pixel: more is smoother and slower.

export-title = Save the view as a picture
export-window = PNG, window size…
export-double = PNG, twice the window…
export-large = PNG, 4096 px…
png-file-type = PNG picture
export-error = Could not save the picture: { $reason }
toolbar-presentation = Presentation
toolbar-presentation-hint = Hide every panel and show only the view (P). P or Esc brings them back.
