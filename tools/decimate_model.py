# Simplifies a glTF model to a target triangle count.
# Usage: python3 decimate_model.py SOURCE.glb OUT.glb TRIANGLES [flat|smooth]
# (needs trimesh, fast-simplification, numpy). "flat" (the default) keeps a
# flat normal per face, for planar heads; "smooth" keeps smooth normals.
import sys, numpy as np, trimesh, fast_simplification
src, dst, target = sys.argv[1], sys.argv[2], int(sys.argv[3])
shading = sys.argv[4] if len(sys.argv) > 4 else "flat"
scene = trimesh.load(src, force="scene")
mesh = scene.to_geometry() if hasattr(scene, "to_geometry") else scene.dump(concatenate=True)
mesh.merge_vertices()
print("source", len(mesh.faces), "faces", np.round(mesh.bounds, 3).tolist())
ratio = 1.0 - target / len(mesh.faces)
v, f = fast_simplification.simplify(mesh.vertices.astype(np.float32), mesh.faces.astype(np.int32), target_reduction=ratio)
out = trimesh.Trimesh(vertices=v, faces=f, process=False)
if shading == "flat":
    # A planar head: every face keeps its own flat normal.
    out.unmerge_vertices()
_ = out.vertex_normals
print("result", len(out.faces), "faces", np.round(out.bounds, 3).tolist())
out.export(dst)
