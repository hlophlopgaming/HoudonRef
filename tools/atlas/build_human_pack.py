#!/usr/bin/env python3
"""Builds the Atlas's human content pack from the Z-Anatomy export.

Input: a directory with `skeletal.glb`, `muscular.glb`, `joints.glb` and
`lexicon.json` from https://github.com/nqwrc/3d-anatomy at commit
8ca3b7421bcfbe88b85859eb1983d5cf79f21749 (`public/models/` and
`public/data/`), exported there from Z-Anatomy.

Output, into `assets/atlas/human/`:

- `atlas.ron`: the manifest. One entry per mesh (where its bytes are, its
  bounds) and one per structure (its Z-Anatomy name, system, tissue, mesh,
  placement and Latin term).
- `<system>.bin`: each mesh's positions, quantised to 16 bits within the
  mesh's bounds, then its triangle indices (16 or 32 bits).

Geometry is only decoded and quantised: no simplification, no reshaping.
Paired structures share one mesh with a mirrored placement, as in the
source. Normals are left out: the app works them out on loading.

Usage: python3 tools/atlas/build_human_pack.py SOURCE_DIR OUT_DIR
Needs: pip install DracoPy numpy
"""

import json
import math
import struct
import sys
from pathlib import Path

SYSTEMS = ["skeletal", "muscular", "joints"]

# Source material names -> tissue kinds.
TISSUES = {
    "Bone-1": "Bone",
    "Cartilage": "Cartilage",
    "Suture-1": "Bone",
    "Suture-2": "Bone",
    "Teeth": "Tooth",
    "Teeth-roots": "Tooth",
    "Dentine": "Tooth",
    "Internal rotator": "Muscle",
    "Trapezius": "Muscle",
    "Diaphragm": "Muscle",
    "Tendon": "Tendon",
    "Fascia": "Fascia",
    "Ligament": "Ligament",
    "Bursa": "Bursa",
    "Articular capsule": "Capsule",
    "Fat": "Fat",
}


def read_glb(path):
    data = Path(path).read_bytes()
    magic, _version, _length = struct.unpack_from("<4sII", data, 0)
    if magic != b"glTF":
        raise ValueError(f"{path}: not a binary glTF file")
    offset = 12
    document, binary = None, b""
    while offset < len(data):
        size, kind = struct.unpack_from("<I4s", data, offset)
        chunk = data[offset + 8 : offset + 8 + size]
        if kind == b"JSON":
            document = json.loads(chunk)
        elif kind == b"BIN\x00":
            binary = chunk
        offset += 8 + size
    return document, binary


def view_bytes(document, binary, index):
    view = document["bufferViews"][index]
    start = view.get("byteOffset", 0)
    return binary[start : start + view["byteLength"]]


def decode_mesh(document, binary, mesh):
    """All primitives of `mesh` as one: positions, triangles and the tissue
    of its largest primitive."""
    import DracoPy

    positions, triangles = [], []
    largest, tissue = -1, None
    for primitive in mesh["primitives"]:
        draco = primitive.get("extensions", {}).get("KHR_draco_mesh_compression")
        if draco is None:
            raise ValueError("expected Draco-compressed primitives")
        decoded = DracoPy.decode(view_bytes(document, binary, draco["bufferView"]))
        points = [tuple(map(float, p)) for p in decoded.points]
        faces = [tuple(map(int, f)) for f in decoded.faces]
        base = len(positions)
        positions.extend(points)
        triangles.extend((a + base, b + base, c + base) for a, b, c in faces)
        if len(faces) > largest:
            largest = len(faces)
            material = document["materials"][primitive.get("material", 0)]["name"]
            tissue = TISSUES.get(material)
            if tissue is None:
                raise ValueError(f"unknown material {material!r}")
    return positions, triangles, tissue


def quaternion_matrix(q):
    x, y, z, w = q
    return [
        [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
        [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
        [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
    ]


def local_matrix(node):
    """The node's placement as a 4 x 4 row-major matrix."""
    if "matrix" in node:
        m = node["matrix"]  # column-major in glTF
        return [[m[c * 4 + r] for c in range(4)] for r in range(4)]
    t = node.get("translation", [0.0, 0.0, 0.0])
    r = quaternion_matrix(node.get("rotation", [0.0, 0.0, 0.0, 1.0]))
    s = node.get("scale", [1.0, 1.0, 1.0])
    return [
        [r[0][0] * s[0], r[0][1] * s[1], r[0][2] * s[2], t[0]],
        [r[1][0] * s[0], r[1][1] * s[1], r[1][2] * s[2], t[1]],
        [r[2][0] * s[0], r[2][1] * s[1], r[2][2] * s[2], t[2]],
        [0.0, 0.0, 0.0, 1.0],
    ]


def multiply(a, b):
    return [[sum(a[r][k] * b[k][c] for k in range(4)) for c in range(4)] for r in range(4)]


def world_matrices(document):
    """Each node's world placement, and its parent node's index."""
    parents = {}
    for index, node in enumerate(document["nodes"]):
        for child in node.get("children", []):
            parents[child] = index
    world = {}

    def resolve(index):
        if index not in world:
            local = local_matrix(document["nodes"][index])
            parent = parents.get(index)
            world[index] = local if parent is None else multiply(resolve(parent), local)
        return world[index]

    for index in range(len(document["nodes"])):
        resolve(index)
    return world, parents


def quantise(positions):
    lo = [min(p[a] for p in positions) for a in range(3)]
    hi = [max(p[a] for p in positions) for a in range(3)]
    span = [max(h - l, 1e-9) for l, h in zip(lo, hi)]
    out = bytearray()
    for p in positions:
        for a in range(3):
            out += struct.pack("<H", round((p[a] - lo[a]) / span[a] * 65535))
    return bytes(out), lo, hi


def pad4(blob):
    blob += b"\0" * ((4 - len(blob) % 4) % 4)


def ron_string(text):
    return json.dumps(text, ensure_ascii=False)


def ron_floats(values):
    return "(" + ", ".join(repr(float(v)) for v in values) + ")"


def build(source, out):
    source, out = Path(source), Path(out)
    out.mkdir(parents=True, exist_ok=True)
    lexicon = json.loads((source / "lexicon.json").read_text())
    meshes, structures = [], []
    for system in SYSTEMS:
        document, binary = read_glb(source / f"{system}.glb")
        world, parents = world_matrices(document)
        blob = bytearray()
        mesh_ids = {}
        tissues = {}
        for index, node in enumerate(document["nodes"]):
            if "mesh" not in node:
                continue
            source_mesh = node["mesh"]
            if source_mesh not in mesh_ids:
                positions, triangles, tissue = decode_mesh(
                    document, binary, document["meshes"][source_mesh]
                )
                quantised, lo, hi = quantise(positions)
                wide = len(positions) > 65535
                offset = len(blob)
                blob += quantised
                pad4(blob)
                fmt = "<I" if wide else "<H"
                for triangle in triangles:
                    for i in triangle:
                        blob += struct.pack(fmt, i)
                pad4(blob)
                mesh_ids[source_mesh] = len(meshes)
                tissues[source_mesh] = tissue
                meshes.append(
                    {
                        "file": f"{system}.bin",
                        "offset": offset,
                        "vertices": len(positions),
                        "triangles": len(triangles),
                        "wide": wide,
                        "min": lo,
                        "max": hi,
                    }
                )
            name = node.get("extras", {}).get("za_name", node.get("name"))
            base = name[:-2] if name.endswith((".l", ".r")) else name
            latin = lexicon.get(base, {}).get("la")
            parent = parents.get(index)
            parent_name = None
            if parent is not None:
                parent_node = document["nodes"][parent]
                parent_name = parent_node.get("extras", {}).get("za_name", parent_node.get("name"))
            matrix = world[index]
            structures.append(
                {
                    "id": name,
                    "system": system.capitalize(),
                    "tissue": tissues[source_mesh],
                    "mesh": mesh_ids[source_mesh],
                    "matrix": [matrix[r][c] for c in range(4) for r in range(4)],
                    "latin": latin,
                    "parent": parent_name,
                }
            )
        (out / f"{system}.bin").write_bytes(bytes(blob))

    lines = [
        "// The Atlas's human content: built by tools/atlas/build_human_pack.py",
        "// from Z-Anatomy (CC BY-SA 4.0) over BodyParts3D (CC BY 4.0).",
        "// See LICENSE and NOTICE next to this file.",
        "(",
        "    version: 1,",
        "    meshes: [",
    ]
    for m in meshes:
        lines.append(
            f"        (file: {ron_string(m['file'])}, offset: {m['offset']}, "
            f"vertices: {m['vertices']}, triangles: {m['triangles']}, "
            f"wide: {str(m['wide']).lower()}, min: {ron_floats(m['min'])}, "
            f"max: {ron_floats(m['max'])}),"
        )
    lines.append("    ],")
    lines.append("    structures: [")
    for s in structures:
        latin = f"Some({ron_string(s['latin'])})" if s["latin"] else "None"
        parent = f"Some({ron_string(s['parent'])})" if s["parent"] else "None"
        lines.append(
            f"        (id: {ron_string(s['id'])}, system: {s['system']}, "
            f"tissue: {s['tissue']}, mesh: {s['mesh']}, "
            f"matrix: {ron_floats(s['matrix'])}, latin: {latin}, parent: {parent}),"
        )
    lines.append("    ],")
    lines.append(")")
    (out / "atlas.ron").write_text("\n".join(lines) + "\n")
    return meshes, structures


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    meshes, structures = build(sys.argv[1], sys.argv[2])
    print(f"{len(structures)} structures, {len(meshes)} meshes")
