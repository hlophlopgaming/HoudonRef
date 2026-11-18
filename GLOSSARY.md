# HoudonRef

A tool for artists who learn form, light and anatomy by drawing from 3D models, either straight from the screen or from saved reference images.

## Sections

**Atlas**:
The section for studying anatomy layer by layer, from skin to skeleton.
_Avoid_: Anatomy viewer, encyclopedia

**Studio**:
The section where the artist sets up a model, its lights, room and camera to draw from.
_Avoid_: Viewer, scene editor

**Atelier**:
The section for timed drawing sessions that change the subject, angle and light between rounds.
_Avoid_: Trainer, practice mode

## Studio

**Model**:
A 3D asset loaded from a file, with its own shape and materials.
_Avoid_: Mesh, asset, file (in prose)

**Object**:
One placement of a model in the Studio scene, with its own position, rotation and size. Two apples on a table are one model and two objects.
_Avoid_: Instance, item, prop

**Figure**:
A model of a human body, shown at real human height.
_Avoid_: Character, avatar, mannequin

**Light**:
One source of light in the Studio: directional, point, spot or area.
_Avoid_: Lamp, light source (in prose), emitter

**Light mask**:
An image a spot light projects to shape its beam, such as window bars or foliage.
_Avoid_: Gobo, cookie, light texture

**Light preset**:
A named arrangement of lights, without room or camera.
_Avoid_: Lighting setup, light scheme

**Room**:
What surrounds the objects: the floor, the backdrop and the ambient light they give.
_Avoid_: Environment, background

**Camera preset**:
A named camera position and lens, relative either to the whole scene or to the selected object.
_Avoid_: Viewpoint, bookmark, saved view

**Scene**:
Everything set up in the Studio, saved together: the objects, the lights, the room, the camera and the shading mode.
_Avoid_: Project, document, setup

## Viewing

**Shading mode**:
How the Studio draws its objects: Solid, Lit or Render.
_Avoid_: View mode, render mode (for the first two)

**Solid**:
A shading mode that ignores the scene's lights and shows form with a built-in light, a matcap or flat colour.
_Avoid_: Workbench, preview

**Lit**:
A shading mode that shows objects under the scene's lights and room, with shadows. The main mode for drawing.
_Avoid_: Material preview, realtime

**Render**:
A shading mode that refines a still view over a few seconds into a picture with bounce light and soft shadows.
_Avoid_: Path tracing (in the interface), final render

**Clay**:
A single plain colour that stands in for every material, so only form and light are seen.
_Avoid_: Grey material, neutral material

**Matcap**:
A picture of a lit sphere used to shade objects in the Solid mode, independent of the scene's lights.
_Avoid_: Material capture, lit sphere

**Overlay**:
A drawing aid laid over Solid or Lit: cavity, contour lines or wireframe.
_Avoid_: Effect, filter

**Cavity**:
An overlay that brightens ridges and darkens hollows to make small changes of form readable.
_Avoid_: Curvature shading, edge highlight

**Contour lines**:
An overlay that draws an object's silhouette and the inner folds of its form as lines.
_Avoid_: Outline (means silhouette only), edges, line art

**Reflex**:
Light bounced back into the shadow side of a form from the floor or nearby objects.
_Avoid_: Bounce light, indirect light (in the interface)
