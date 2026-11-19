#!/usr/bin/env python3
"""Writes assets/models/test/two_materials.glb, a test model made here (CC0).

One mesh with two primitives, side by side along X, 40 cm cubes:
- "red": a plain red material (base colour factor only), rough;
- "checker": a green and white checker base colour texture with a flat
  normal map, half metallic.

Run from the repository root: python3 tools/make_test_materials_model.py
"""

import json
import struct
import zlib
from pathlib import Path


def png(width, height, pixels):
    """Encodes RGBA8 rows as a PNG file."""
    raw = b"".join(b"\x00" + bytes(pixels[y * width * 4:(y + 1) * width * 4]) for y in range(height))

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


def cube(cx, size):
    """Positions, normals, uvs and indices of a cube centred at (cx, size/2, 0)."""
    h = size / 2
    faces = [
        ((1, 0, 0), (0, 0, -1), (0, 1, 0)),
        ((-1, 0, 0), (0, 0, 1), (0, 1, 0)),
        ((0, 1, 0), (1, 0, 0), (0, 0, -1)),
        ((0, -1, 0), (1, 0, 0), (0, 0, 1)),
        ((0, 0, 1), (1, 0, 0), (0, 1, 0)),
        ((0, 0, -1), (-1, 0, 0), (0, 1, 0)),
    ]
    positions, normals, uvs, indices = [], [], [], []
    for n, u, v in faces:
        base = len(positions)
        for su, sv in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
            p = [n[i] * h + u[i] * h * su + v[i] * h * sv for i in range(3)]
            positions.append((p[0] + cx, p[1] + h, p[2]))
            normals.append(n)
            uvs.append(((su + 1) / 2, (1 - sv) / 2))
        indices += [base, base + 1, base + 2, base, base + 2, base + 3]
    return positions, normals, uvs, indices


def main():
    out = Path("assets/models/test/two_materials.glb")
    checker = []
    for y in range(4):
        for x in range(4):
            checker += [40, 160, 60, 255] if (x + y) % 2 else [240, 240, 240, 255]
    flat_normal = [128, 128, 255, 255] * 4
    images = [png(4, 4, checker), png(2, 2, flat_normal)]

    binary = bytearray()
    views, accessors = [], []

    def add_view(data, target=None):
        while len(binary) % 4:
            binary.append(0)
        views.append({"buffer": 0, "byteOffset": len(binary), "byteLength": len(data), **({"target": target} if target else {})})
        binary.extend(data)
        return len(views) - 1

    def add_accessor(values, kind, component, target):
        flat = [c for v in values for c in (v if isinstance(v, (tuple, list)) else (v,))]
        fmt = "<" + ("f" if component == 5126 else "I") * len(flat)
        view = add_view(struct.pack(fmt, *flat), target)
        acc = {"bufferView": view, "componentType": component, "count": len(values), "type": kind}
        if kind == "VEC3" and component == 5126:
            acc["min"] = [min(v[i] for v in values) for i in range(3)]
            acc["max"] = [max(v[i] for v in values) for i in range(3)]
        accessors.append(acc)
        return len(accessors) - 1

    primitives = []
    for material, cx in ((0, -0.3), (1, 0.3)):
        positions, normals, uvs, indices = cube(cx, 0.4)
        primitives.append({
            "attributes": {
                "POSITION": add_accessor(positions, "VEC3", 5126, 34962),
                "NORMAL": add_accessor(normals, "VEC3", 5126, 34962),
                "TEXCOORD_0": add_accessor(uvs, "VEC2", 5126, 34962),
            },
            "indices": add_accessor(indices, "SCALAR", 5125, 34963),
            "material": material,
        })
    image_views = [add_view(data) for data in images]
    while len(binary) % 4:
        binary.append(0)

    document = {
        "asset": {"version": "2.0", "generator": "HoudonRef tools/make_test_materials_model.py"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0, "name": "two_materials"}],
        "meshes": [{"name": "two_materials", "primitives": primitives}],
        "materials": [
            {"name": "red", "pbrMetallicRoughness": {"baseColorFactor": [0.8, 0.05, 0.05, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.7}},
            {
                "name": "checker",
                "pbrMetallicRoughness": {"baseColorTexture": {"index": 0}, "metallicFactor": 0.5, "roughnessFactor": 0.4},
                "normalTexture": {"index": 1},
            },
        ],
        "textures": [{"source": 0, "sampler": 0}, {"source": 1, "sampler": 0}],
        "samplers": [{"magFilter": 9729, "minFilter": 9987}],
        "images": [{"bufferView": image_views[0], "mimeType": "image/png"}, {"bufferView": image_views[1], "mimeType": "image/png"}],
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"byteLength": len(binary)}],
    }
    json_bytes = json.dumps(document, separators=(",", ":")).encode()
    json_bytes += b" " * (-len(json_bytes) % 4)
    glb = struct.pack("<4sII", b"glTF", 2, 12 + 8 + len(json_bytes) + 8 + len(binary))
    glb += struct.pack("<I4s", len(json_bytes), b"JSON") + json_bytes
    glb += struct.pack("<I4s", len(binary), b"BIN\x00") + bytes(binary)
    out.write_bytes(glb)
    print(f"wrote {out} ({len(glb)} bytes)")


if __name__ == "__main__":
    main()
