"""Round trip of the pack builder on a tiny Draco-compressed glTF.

Run: python3 -m unittest discover -s tools/atlas
"""

import json
import struct
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import build_human_pack  # noqa: E402

CUBE = [(x, y, z) for x in (0.0, 0.1) for y in (0.0, 0.2) for z in (0.0, 0.3)]
FACES = [
    (0, 1, 3), (0, 3, 2), (4, 6, 7), (4, 7, 5), (0, 4, 5), (0, 5, 1),
    (2, 3, 7), (2, 7, 6), (0, 2, 6), (0, 6, 4), (1, 5, 7), (1, 7, 3),
]


def glb(material, nodes):
    import DracoPy

    draco = DracoPy.encode(CUBE, faces=FACES, quantization_bits=16)
    padded = draco + b"\0" * ((4 - len(draco) % 4) % 4)
    document = {
        "asset": {"version": "2.0"},
        "buffers": [{"byteLength": len(padded)}],
        "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": len(draco)}],
        "accessors": [
            {"componentType": 5125, "count": 36, "type": "SCALAR"},
            {"componentType": 5126, "count": 8, "type": "VEC3"},
        ],
        "materials": [{"name": material}],
        "meshes": [
            {
                "primitives": [
                    {
                        "attributes": {"POSITION": 1},
                        "indices": 0,
                        "material": 0,
                        "extensions": {
                            "KHR_draco_mesh_compression": {
                                "bufferView": 0,
                                "attributes": {"POSITION": 0},
                            }
                        },
                    }
                ]
            }
        ],
        "nodes": nodes,
        "scenes": [{"nodes": list(range(len(nodes)))}],
    }
    text = json.dumps(document).encode()
    text += b" " * ((4 - len(text) % 4) % 4)
    body = struct.pack("<I4s", len(text), b"JSON") + text
    body += struct.pack("<I4s", len(padded), b"BIN\x00") + padded
    return struct.pack("<4sII", b"glTF", 2, 12 + len(body)) + body


class RoundTrip(unittest.TestCase):
    def test_a_draco_mesh_comes_back_with_its_vertices_and_bounds(self):
        with tempfile.TemporaryDirectory() as tmp:
            source, out = Path(tmp) / "source", Path(tmp) / "out"
            source.mkdir()
            pair = [
                {"name": "Bone.l", "extras": {"za_name": "Bone.l"}, "mesh": 0,
                 "translation": [0.2, 0, 0], "scale": [-1, 1, 1]},
                {"name": "Bone.r", "extras": {"za_name": "Bone.r"}, "mesh": 0,
                 "translation": [-0.2, 0, 0]},
            ]
            (source / "skeletal.glb").write_bytes(glb("Bone-1", pair))
            (source / "muscular.glb").write_bytes(
                glb("Internal rotator", [{"name": "Muscle", "mesh": 0}])
            )
            (source / "joints.glb").write_bytes(glb("Ligament", [{"name": "Lig", "mesh": 0}]))
            (source / "lexicon.json").write_text(json.dumps({"Bone": {"la": "Os"}}))

            meshes, structures = build_human_pack.build(source, out)

            self.assertEqual(len(meshes), 3, "the pair shares one mesh")
            self.assertEqual([s["id"] for s in structures], ["Bone.l", "Bone.r", "Muscle", "Lig"])
            self.assertEqual(structures[0]["latin"], "Os")
            self.assertEqual(structures[2]["tissue"], "Muscle")
            mesh = meshes[0]
            self.assertEqual((mesh["vertices"], mesh["triangles"]), (8, 12))
            for got, want in zip(mesh["min"] + mesh["max"], [0, 0, 0, 0.1, 0.2, 0.3]):
                self.assertAlmostEqual(got, want, places=4)
            data = (out / "skeletal.bin").read_bytes()
            positions = struct.unpack_from("<24H", data, 0)
            self.assertEqual(max(positions), 65535)
            self.assertEqual(min(positions), 0)
            indices = struct.unpack_from("<36H", data, 48)
            self.assertEqual(sorted(set(indices)), list(range(8)))
            self.assertIn("structures: [", (out / "atlas.ron").read_text())


if __name__ == "__main__":
    unittest.main()
