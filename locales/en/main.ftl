# HoudonRef user interface, English (source language).

app-title = HoudonRef
section-atlas = Atlas
section-studio = Studio
section-atelier = Atelier
language = Language

atlas-coming = The anatomy atlas is coming: an écorché by layers, from skin to skeleton, with names in English, Russian and Latin.
atelier-coming = The drawing atelier is coming: timed sessions that go from a Loomis ball to planes to a detailed head, with changing angles and light.

model-stats = { $triangles } triangles

studio-objects = Objects
studio-add-male = Add male figure (test)
studio-add-female = Add female figure (test)
studio-object-duplicate = Duplicate
studio-object-delete = Delete
studio-frame-scene = Frame scene
studio-frame-selected = Frame selected
studio-selected-object = Selected object
studio-no-selection = Click an object in the view or in the list to select it.
studio-position = Position
studio-rotation = Rotation
studio-size = Size
studio-camera-hint = Drag to orbit, right-drag or Shift-drag to pan, scroll to move closer. Click to select, double-click to frame, Delete to remove.
studio-drop-hint = Drop a .glb or .gltf file onto the window to add it.

studio-camera = Camera
studio-focal-length = Focal length

studio-light-azimuth = Direction
studio-light-elevation = Height
studio-light-intensity = Intensity

error-blend-file = .blend files cannot be opened directly yet. In Blender, use File → Export → glTF 2.0 (.glb) and open that file.
error-unsupported-file = Unsupported file type: { $name }. Drop a model (.glb or .gltf), an image, or a scene (.ron).
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
studio-figure = Human figure (shown at figure height)
studio-dimensions = Width { $width }, height { $height }, depth { $depth }
studio-real-size = Real size (largest side)
studio-real-size-warning = The size in this file looks wrong (smaller than { $smallest } or larger than { $largest }). Enter the real size.
unit-centimetre-short = cm
unit-inch-short = in
unit-foot-short = ft
length = { $value } { $unit }

studio-lights = Lights
studio-add-light = + { $kind }
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
values-hint = V in the viewport turns the study on and off.

studio-add-asaro-head = Add Asaro head
studio-add-planar-head = Add planar head
model-asaro-head = Asaro head
model-planar-head = planar head
studio-add-base-head = Add base head
model-base-head = base head
studio-add-wolfsaro-head = Add Wolfsaro head (canine)
model-wolfsaro-head = Wolfsaro head
