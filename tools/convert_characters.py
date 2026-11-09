#!/usr/bin/env python3
"""Converts the Quaternius "Ultimate Animated Character Pack" glTF files into
`assets/characters/characters.bin`, the compact format `crates/client/src/character.rs`
loads: one shared skeleton, a few animation clips resampled at 30 fps, and the meshes.

Usage: python3 -I tools/convert_characters.py <dir with .gltf files> assets/characters/characters.bin

Format (little endian):
  b"TWC1"
  u8 bones; per bone: str name, u8 parent (255 = root), 16 f32 inverse bind matrix (column major),
                       10 f32 rest pose (translation xyz, rotation xyzw, scale xyz)
  u8 clips; per clip: str name, f32 duration, u16 frames,
                      frames * bones * 7 f32 (translation xyz, rotation xyzw)
  u8 models; per model: str name, u8 materials, per material: str name, 3 u8 sRGB colour
             u16 skin vertices (one per spot); per vertex: 3 f32 position,
                 3 i8 outline normal (averaged over the spot), 4 u8 joints, 4 u8 weights (sum 255)
             u16 vertices; per vertex: u16 skin vertex, 3 i8 normal, u8 material
             u32 indices; u16 each
Vertices sharing a spot and a material are welded, and their normals are smoothed unless
they are more than SMOOTH_DEG apart, which keeps the meshes small for CPU skinning.
  str = u8 length + UTF-8 bytes
"""
import base64, json, math, os, struct, sys

MODELS = ["BlueSoldier_Male", "Soldier_Male", "Pirate_Male", "Ninja_Male", "Ninja_Sand", "Knight_Male", "Knight_Golden_Male", "Goblin_Male", "Elf", "Kimono_Male", "Worker_Male", "Wizard"]
CLIPS = ["Idle", "Walk", "Run"]
ANIM_SOURCE = "Soldier_Male"
FPS = 30.0
SMOOTH_DEG = 75.0
# skin below this height (model units) is a boot
BOOT_TOP = 0.3


def load(path):
    g = json.load(open(path))
    uri = g["buffers"][0]["uri"]
    buf = base64.b64decode(uri.split(",", 1)[1])

    def acc(i):
        a = g["accessors"][i]
        bv = g["bufferViews"][a["bufferView"]]
        n = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}[a["type"]]
        f = {5126: "f", 5123: "H", 5121: "B", 5125: "I"}[a["componentType"]]
        sz = struct.calcsize(f)
        off = bv.get("byteOffset", 0) + a.get("byteOffset", 0)
        st = bv.get("byteStride", sz * n)
        out = [struct.unpack_from("<" + f * n, buf, off + k * st) for k in range(a["count"])]
        if a.get("normalized") and f == "B":
            out = [tuple(x / 255.0 for x in v) for v in out]
        return out

    return g, acc


def to_srgb(c):
    c = max(0.0, min(1.0, c))
    s = c * 12.92 if c <= 0.0031308 else 1.055 * c ** (1 / 2.4) - 0.055
    return int(round(s * 255))


def slerp(a, b, t):
    d = sum(x * y for x, y in zip(a, b))
    if d < 0:
        b = [-x for x in b]
        d = -d
    if d > 0.9995:
        r = [x + (y - x) * t for x, y in zip(a, b)]
    else:
        th = math.acos(d)
        s = math.sin(th)
        wa, wb = math.sin((1 - t) * th) / s, math.sin(t * th) / s
        r = [x * wa + y * wb for x, y in zip(a, b)]
    n = math.sqrt(sum(x * x for x in r))
    return [x / n for x in r]


def sample(times, values, t, rot):
    if t <= times[0]:
        return list(values[0])
    if t >= times[-1]:
        return list(values[-1])
    k = 0
    while times[k + 1] < t:
        k += 1
    f = (t - times[k]) / (times[k + 1] - times[k])
    a, b = values[k], values[k + 1]
    return slerp(a, b, f) if rot else [x + (y - x) * f for x, y in zip(a, b)]


def pstr(s):
    b = s.encode()
    return struct.pack("<B", len(b)) + b


def i8(v):
    return max(-127, min(127, int(round(v * 127))))


def main(src, dst):
    out = bytearray(b"TWC1")
    g, acc = load(os.path.join(src, ANIM_SOURCE + ".gltf"))
    skin = g["skins"][0]
    joints = skin["joints"]
    jindex = {n: i for i, n in enumerate(joints)}
    parent_node = {}
    for i, n in enumerate(g["nodes"]):
        for c in n.get("children", []):
            parent_node[c] = i
    ibm = acc(skin["inverseBindMatrices"])
    out += struct.pack("<B", len(joints))
    rest = []
    for i, n in enumerate(joints):
        node = g["nodes"][n]
        p = parent_node.get(n)
        out += pstr(node["name"]) + struct.pack("<B", jindex.get(p, 255))
        out += struct.pack("<16f", *ibm[i])
        r = list(node.get("translation", [0, 0, 0])) + list(node.get("rotation", [0, 0, 0, 1]))
        r += list(node.get("scale", [1, 1, 1]))
        rest.append(r)
        out += struct.pack("<10f", *r)

    anims = {a["name"]: a for a in g["animations"]}
    out += struct.pack("<B", len(CLIPS))
    for name in CLIPS:
        a = anims[name]
        tracks = {}
        dur = 0.0
        for c in a["channels"]:
            node = c["target"]["node"]
            if node not in jindex:
                continue
            s = a["samplers"][c["sampler"]]
            times = [t[0] for t in acc(s["input"])]
            dur = max(dur, times[-1])
            tracks[(jindex[node], c["target"]["path"])] = (times, acc(s["output"]))
        frames = int(round(dur * FPS)) + 1
        out += pstr(name) + struct.pack("<fH", dur, frames)
        for f in range(frames):
            t = min(dur, f / FPS)
            for j in range(len(joints)):
                tr = tracks.get((j, "translation"))
                ro = tracks.get((j, "rotation"))
                tv = sample(*tr, t, False) if tr else rest[j][0:3]
                rv = sample(*ro, t, True) if ro else rest[j][3:7]
                out += struct.pack("<7f", *tv, *rv)

    out += struct.pack("<B", len(MODELS))
    for name in MODELS:
        g, acc = load(os.path.join(src, name + ".gltf"))
        assert g["skins"][0]["joints"] == joints, name
        mats = [
            (m["name"], m.get("pbrMetallicRoughness", {}).get("baseColorFactor", [1, 1, 1, 1]))
            for m in g["materials"]
        ]
        # the pack's skin is nearly black, so bare feet read as boots there; with light
        # skin they need a material of their own
        skin_mat = [m[0] for m in mats].index("Skin")
        boots = len(mats)
        mats.append(("Boots", [0.03, 0.025, 0.02, 1]))
        out += pstr(name) + struct.pack("<B", len(mats))
        for mname, c in mats:
            out += pstr(mname) + struct.pack("<3B", *(to_srgb(x) for x in c[:3]))
        verts, idx = [], []
        for prim in g["meshes"][0]["primitives"]:
            at = prim["attributes"]
            base = len(verts)
            pos, nrm = acc(at["POSITION"]), acc(at["NORMAL"])
            jo, we = acc(at["JOINTS_0"]), acc(at["WEIGHTS_0"])
            for k in range(len(pos)):
                w = [round(x * 255) for x in we[k]]
                w[w.index(max(w))] += 255 - sum(w)
                mat = prim["material"]
                if mat == skin_mat and pos[k][1] < BOOT_TOP:
                    mat = boots
                verts.append([pos[k], nrm[k], mat, jo[k], w])
            idx += [base + i[0] for i in acc(prim["indices"])]
        spots, skin, fill, fill_of = {}, [], [], {}
        cos_lim = math.cos(math.radians(SMOOTH_DEG))
        remap = []
        for v in verts:
            key = tuple(round(x, 4) for x in v[0])
            if key not in spots:
                spots[key] = len(skin)
                skin.append([v[0], [0.0, 0.0, 0.0], v[3], v[4]])
            si = spots[key]
            for i in range(3):
                skin[si][1][i] += v[1][i]
            # weld with an earlier vertex of the same spot and material if the normals agree
            cands = fill_of.setdefault((si, v[2]), [])
            hit = None
            for fi in cands:
                n = fill[fi][1]
                l = math.sqrt(sum(x * x for x in n)) or 1.0
                if sum(a * b for a, b in zip(n, v[1])) / l > cos_lim:
                    hit = fi
                    break
            if hit is None:
                hit = len(fill)
                fill.append([si, list(v[1]), v[2]])
                cands.append(hit)
            else:
                for i in range(3):
                    fill[hit][1][i] += v[1][i]
            remap.append(hit)
        idx = [remap[i] for i in idx]
        tris = [idx[k:k + 3] for k in range(0, len(idx), 3)]
        idx = [i for t in tris if len(set(t)) == 3 for i in t]

        def unit(n):
            l = math.sqrt(sum(x * x for x in n)) or 1.0
            return [x / l for x in n]

        assert len(fill) < 65536, name
        out += struct.pack("<H", len(skin))
        for p, on, jo, w in skin:
            out += struct.pack("<3f", *p)
            out += struct.pack("<3b", *(i8(x) for x in unit(on)))
            out += struct.pack("<4B4B", *jo, *w)
        out += struct.pack("<H", len(fill))
        for si, n, mat in fill:
            out += struct.pack("<H3bB", si, *(i8(x) for x in unit(n)), mat)
        out += struct.pack("<I", len(idx))
        out += struct.pack("<%dH" % len(idx), *idx)
        print(name, "spots", len(skin), "vertices", len(fill), "triangles", len(idx) // 3)
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    open(dst, "wb").write(out)
    print(dst, len(out), "bytes")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
