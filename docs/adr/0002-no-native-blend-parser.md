# No native .blend parser

HoudonRef does not read `.blend` files itself. The format is a memory dump whose layout changes between Blender versions (5.x files even change the header and may be zstd-compressed), and modifiers, rigs and shape keys only exist as data that Blender itself evaluates, so a native reader would show a "nearly right" model. On desktop, `.blend` files are converted through an installed Blender running headless and exporting glTF; on the web and Android, the app explains how to export glTF from Blender.
