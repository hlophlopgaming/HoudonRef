# Studio matcaps

The 27 presets in `blender/` come from Blender 5.3's
[`studiolights/matcap` directory](https://github.com/blender/blender/tree/main/release/datafiles/studiolights/matcap).
The Blender community released them under CC0 or into the public domain.
The [original license notice](blender/license.txt) accompanies the images.

HoudonRef adds the EXR diffuse and specular RGB layers in linear light,
averages each 2 × 2 pixel block, then clamps the result to the display range
and encodes it as a 256 × 256 sRGB PNG. The single-layer normal-check preset
uses its RGB channels directly before resizing. Solid mode tints the baked
image with the selected object color, including the baked highlights.
