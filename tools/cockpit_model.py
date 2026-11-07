# Modélisation du cockpit du Darkstar dans Blender (script sans interface) :
#   /Applications/Blender.app/Contents/MacOS/Blender -b --python tools/cockpit_model.py
# Produit assets/cockpit.obj + assets/cockpit.mtl (couleurs par sommet = occlusion ambiante cuite par
# Cycles). Repère du jeu : x droite, y haut, z avant (mètres, œil à l'origine) ; Blender : X droite,
# Y avant, Z haut. Les emplacements des instruments reprennent les fonctions de src/cockpit3d.rs.

import bpy
import bmesh
import math
import os
from mathutils import Vector

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets")
os.makedirs(OUT, exist_ok=True)


def V(x, y, z):
    """Repère du jeu -> Blender."""
    return Vector((x, z, y))


# ---------- Scène vide ----------
bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene

# ---------- Matériaux (nom = case de l'atlas du jeu ; préfixe E_ = émissif) ----------
MATS = {
    "Metal": (0.055, 0.058, 0.063),
    "Panel": (0.07, 0.074, 0.078),
    "Ribs": (0.05, 0.052, 0.056),
    "Console": (0.06, 0.063, 0.066),
    "Floor": (0.04, 0.04, 0.042),
    "Black": (0.015, 0.015, 0.017),
    "Chrome": (0.55, 0.56, 0.58),
    "White": (0.85, 0.86, 0.88),
    "Red": (0.62, 0.04, 0.03),
    "Yellow": (0.85, 0.65, 0.05),
    "Glass": (0.01, 0.012, 0.016),
    "E_Teal": (0.35, 0.95, 0.85),
    "E_Green": (0.45, 1.0, 0.35),
    "E_Amber": (1.0, 0.7, 0.15),
}
mats = {}
for name, rgb in MATS.items():
    m = bpy.data.materials.new(name)
    m.diffuse_color = (*rgb, 1.0)
    m.use_nodes = True
    bsdf = m.node_tree.nodes.get("Principled BSDF")
    if bsdf:
        bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    mats[name] = m

objects = []


def finish(obj, mat, bevel=0.0, seg=2, smooth=True):
    obj.data.materials.append(mats[mat])
    if bevel > 0:
        b = obj.modifiers.new("Bevel", "BEVEL")
        b.width = bevel
        b.segments = seg
        b.limit_method = "ANGLE"
        b.angle_limit = math.radians(35)
    if smooth and hasattr(obj.data, "polygons"):
        for p in obj.data.polygons:
            p.use_smooth = True
    objects.append(obj)
    return obj


def mesh_obj(name, verts, faces, mat, bevel=0.0, seg=2, smooth=False):
    me = bpy.data.meshes.new(name)
    me.from_pydata([V(*p) for p in verts], [], faces)
    me.update()
    ob = bpy.data.objects.new(name, me)
    scene.collection.objects.link(ob)
    # Normales sortantes cohérentes
    bm = bmesh.new()
    bm.from_mesh(me)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(me)
    bm.free()
    return finish(ob, mat, bevel, seg, smooth)


def box(name, a, b, mat, bevel=0.004, seg=2):
    (x0, y0, z0), (x1, y1, z1) = a, b
    verts = [(x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0), (x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1)]
    faces = [(0, 1, 2, 3), (4, 7, 6, 5), (0, 4, 5, 1), (1, 5, 6, 2), (2, 6, 7, 3), (3, 7, 4, 0)]
    return mesh_obj(name, verts, faces, mat, bevel, seg)


def prism(name, quad, depth, mat, bevel=0.003, seg=2):
    """Plaque épaisse à partir d'un quadrilatère (repère jeu), extrudée vers l'œil (-z)."""
    p = [Vector(q) for q in quad]
    n = (p[1] - p[0]).cross(p[3] - p[0]).normalized()
    if n.z > 0:
        n = -n
    top = [q + n * depth for q in p]
    verts = [tuple(q) for q in p] + [tuple(q) for q in top]
    faces = [(0, 1, 2, 3), (4, 7, 6, 5), (0, 4, 5, 1), (1, 5, 6, 2), (2, 6, 7, 3), (3, 7, 4, 0)]
    return mesh_obj(name, verts, faces, mat, bevel, seg)


def tube(name, pts, r, mat, res=10):
    cu = bpy.data.curves.new(name, "CURVE")
    cu.dimensions = "3D"
    cu.bevel_depth = r
    cu.bevel_resolution = max(2, res // 4)
    cu.use_fill_caps = True
    sp = cu.splines.new("NURBS")
    sp.points.add(len(pts) - 1)
    for i, p in enumerate(pts):
        v = V(*p)
        sp.points[i].co = (v.x, v.y, v.z, 1.0)
    sp.order_u = min(4, len(pts))
    sp.use_endpoint_u = True
    sp.resolution_u = 6
    ob = bpy.data.objects.new(name, cu)
    scene.collection.objects.link(ob)
    return finish(ob, mat, 0.0, smooth=False)


def cyl(name, center, axis, r, depth, mat, verts=20, bevel=0.002):
    bpy.ops.mesh.primitive_cylinder_add(vertices=verts, radius=r, depth=depth, location=V(*center))
    ob = bpy.context.active_object
    ob.name = name
    ax = V(*axis).normalized()
    ob.rotation_mode = "QUATERNION"
    ob.rotation_quaternion = Vector((0, 0, 1)).rotation_difference(ax)
    return finish(ob, mat, bevel, 2)


def sphere(name, center, r, mat):
    bpy.ops.mesh.primitive_uv_sphere_add(segments=10, ring_count=6, radius=r, location=V(*center))
    ob = bpy.context.active_object
    ob.name = name
    return finish(ob, mat)


def torus(name, center, axis, R, r, mat, seg=24, arc=(0.0, 1.0)):
    # Tore (ou portion) construit sommet par sommet
    ax = V(*axis).normalized()
    q = Vector((0, 0, 1)).rotation_difference(ax)
    rs, ts = seg, 8
    a0, a1 = arc
    verts, faces = [], []
    for i in range(rs + 1):
        u = (a0 + (a1 - a0) * i / rs) * 2 * math.pi
        for j in range(ts):
            w = j / ts * 2 * math.pi
            p = Vector(((R + r * math.cos(w)) * math.cos(u), (R + r * math.cos(w)) * math.sin(u), r * math.sin(w)))
            verts.append(q @ p + V(*center))
    for i in range(rs):
        for j in range(ts):
            a = i * ts + j
            b = i * ts + (j + 1) % ts
            faces.append((a, b, b + ts, a + ts))
    me = bpy.data.meshes.new(name)
    me.from_pydata(verts, [], faces)
    me.update()
    ob = bpy.data.objects.new(name, me)
    scene.collection.objects.link(ob)
    return finish(ob, mat)


def spline(ctrl, per):
    out = []
    for i in range(len(ctrl) - 1):
        p0 = Vector(ctrl[max(i - 1, 0)])
        p1, p2 = Vector(ctrl[i]), Vector(ctrl[i + 1])
        p3 = Vector(ctrl[min(i + 2, len(ctrl) - 1)])
        for k in range(per):
            t = k / per
            t2, t3 = t * t, t * t * t
            out.append(tuple(0.5 * (2 * p1 + (p2 - p0) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t2 + (3 * p1 - p0 - 3 * p2 + p3) * t3)))
    out.append(tuple(ctrl[-1]))
    return out


# ---------- Géométrie de référence (identique à src/cockpit3d.rs) ----------
PYR = [(0.07, 0.80, 0.035), (-0.20, 0.66, 0.15), (-0.66, 0.53, 0.27), (-1.0, 0.47, 0.33)]


def pyr_at(y):
    for (y0, z0, h0), (y1, z1, h1) in zip(PYR, PYR[1:]):
        if y1 <= y <= y0:
            t = (y0 - y) / (y0 - y1)
            return z0 + (z1 - z0) * t, h0 + (h1 - h0) * t
    return PYR[-1][1], PYR[-1][2]


def on_panel(x, y, lift=0.006):
    return (x, y, pyr_at(y)[0] - lift)


def on_wing(s, u, y, lift=0.006):
    z, hw = pyr_at(y)
    return (s * (hw + u * 0.20), y - u * 0.04, z + u * 0.04 - lift)


GUARDS = [(-1.0, 0.60, -0.37), (1.0, 0.60, -0.37), (1.0, 0.80, -0.64)]

# ---------- Pyramide centrale (solide chanfreiné) ----------
verts, faces = [], []
for (y, z, h) in PYR:
    back = 0.42
    verts += [(-h, y, z), (h, y, z), (h + 0.14, y, z + back), (-(h + 0.10), y, z + back)]
n = len(PYR)
for i in range(n - 1):
    a, b = i * 4, (i + 1) * 4
    faces.append((a + 0, a + 1, b + 1, b + 0))  # face avant
    faces.append((a + 1, a + 2, b + 2, b + 1))  # flanc droit
    faces.append((a + 3, a + 0, b + 0, b + 3))  # flanc gauche
    faces.append((a + 2, a + 3, b + 3, b + 2))  # arrière
faces.append((0, 3, 2, 1))
last = (n - 1) * 4
faces.append((last, last + 1, last + 2, last + 3))
mesh_obj("pyramide", verts, faces, "Panel", bevel=0.014, seg=3)

# Ailes latérales (plaques épaisses)
for s in (-1.0, 1.0):
    q = [on_wing(s, 0.0, -0.30, 0.0), on_wing(s, 1.0, -0.30, 0.0), on_wing(s, 1.0, -0.70, 0.0), on_wing(s, 0.0, -0.70, 0.0)]
    q = [(x, y, z + 0.008) for (x, y, z) in q]
    prism("aile", q, 0.03, "Panel", bevel=0.008, seg=3)


# ---------- Lunettes d'instruments (cadres en relief) ----------
def bezel(name, c00, c10, c11, c01, w, depth, mat="Metal"):
    """Cadre autour d'un écran (4 coins sur la planche), barres chanfreinées + écran sombre."""
    P = [Vector(c) for c in (c00, c10, c11, c01)]
    for i in range(4):
        a, b = P[i], P[(i + 1) % 4]
        d = (b - a).normalized()
        n = (P[1] - P[0]).cross(P[3] - P[0]).normalized()
        if n.z > 0:
            n = -n
        side = n.cross(d).normalized()
        inner = (P[(i + 2) % 4] + P[(i + 3) % 4]) * 0.5 - (a + b) * 0.5
        if side.dot(inner) > 0:
            side = -side
        q = [a - d * w, b + d * w, b + d * w + side * w, a - d * w + side * w]
        prism(name, [tuple(x) for x in q], depth, mat, bevel=0.003)
    prism(name + "_ecran", [tuple(x) for x in P], 0.002, "Glass", bevel=0.0)


def panel_rect(x0, y0, x1, y1, lift=0.006):
    return on_panel(x0, y0, lift), on_panel(x1, y0, lift), on_panel(x1, y1, lift), on_panel(x0, y1, lift)


def wing_rect(s, u0, y0, u1, y1, lift=0.006):
    return on_wing(s, u0, y0, lift), on_wing(s, u1, y0, lift), on_wing(s, u1, y1, lift), on_wing(s, u0, y1, lift)


bezel("cadre_mfd", *panel_rect(-0.165, -0.22, 0.165, -0.50, 0.004), 0.012, 0.014)
bezel("cadre_radio", *panel_rect(0.022, -0.055, 0.115, -0.145, 0.004), 0.008, 0.010)
bezel("cadre_dsply", *panel_rect(-0.12, -0.145, -0.02, -0.21, 0.004), 0.006, 0.006)
bezel("cadre_moteurs", *wing_rect(-1.0, 0.10, -0.43, 0.95, -0.56, 0.004), 0.008, 0.010)

# Cadran sommital, cadrans CG / H2 (bagues + verre)
for (c, r) in [(on_panel(-0.035, 0.0), 0.028), (on_wing(1.0, 0.32, -0.50), 0.055), (on_wing(1.0, 0.30, -0.66), 0.045)]:
    cyl("bague", c, (0, 0.25, -1.0), r, 0.016, "Metal", verts=32, bevel=0.003)
    cyl("verre", (c[0], c[1], c[2] - 0.009), (0, 0.25, -1.0), r * 0.86, 0.002, "Glass", verts=32, bevel=0.0)

# Molettes DSPLY (crantées)
for k in range(4):
    c = on_panel(-0.105 + k * 0.026, -0.19, 0.012)
    cyl("molette", c, (0, 0.25, -1.0), 0.010, 0.016, "Chrome", verts=16, bevel=0.002)

# Rangée de boutons éclairés sous l'écran, boutons verts sous l'écran moteurs
for k in range(6):
    c = on_panel(-0.10 + k * 0.04, -0.53, 0.0)
    box("bouton", (c[0] - 0.012, c[1] - 0.012, c[2] - 0.012), (c[0] + 0.012, c[1] + 0.012, c[2]), "E_Teal", bevel=0.003)
for k in range(3):
    c = on_wing(-1.0, 0.25 + k * 0.22, -0.62, 0.0)
    box("bouton_vert", (c[0] - 0.014, c[1] - 0.012, c[2] - 0.012), (c[0] + 0.014, c[1] + 0.012, c[2]), "E_Green", bevel=0.003)

# Interrupteurs gardés : socle noir, capot rouge chanfreiné, charnière
for (s, u, y) in GUARDS:
    c = on_wing(s, u, y, 0.0)
    box("socle", (c[0] - 0.026, c[1] - 0.034, c[2] - 0.006), (c[0] + 0.026, c[1] + 0.034, c[2]), "Black", bevel=0.003)
    box("capot", (c[0] - 0.02, c[1] - 0.028, c[2] - 0.026), (c[0] + 0.02, c[1] + 0.028, c[2] - 0.006), "Red", bevel=0.006, seg=3)
    cyl("charniere", (c[0], c[1] + 0.03, c[2] - 0.01), (1, 0, 0), 0.004, 0.044, "Chrome", verts=10)
    box("legende", (c[0] - 0.03, c[1] + 0.04, c[2] - 0.002), (c[0] + 0.03, c[1] + 0.046, c[2]), "E_Green", bevel=0.0)

# Levier de train (bras chromé, roue blanche)
g = on_wing(-1.0, 0.20, -0.70, 0.0)
box("socle_train", (g[0] - 0.03, g[1] - 0.08, g[2] - 0.008), (g[0] + 0.03, g[1] + 0.08, g[2]), "Black", bevel=0.004)
tube("bras_train", [g, (g[0], g[1] + 0.02, g[2] - 0.05), (g[0], g[1] + 0.06, g[2] - 0.07)], 0.006, "Chrome")
cyl("roue_train", (g[0], g[1] + 0.06, g[2] - 0.075), (1, 0, 0), 0.02, 0.012, "White", verts=20, bevel=0.003)

# ---------- Verrière : arceaux tubulaires rivetés, bord supérieur des fenêtres, ciel de cabine ----------
for s in (-1.0, 1.0):
    edge = spline([(s * 0.05, 0.10, 0.82), (s * 0.45, 0.30, 0.65), (s * 1.05, 0.34, 0.25), (s * 1.4, 0.30, -0.4)], 6)
    tube("bord_fenetre", edge, 0.025, "Metal")
    # Ciel de cabine : bande de quadrilatères au-dessus du bord de fenêtre
    vs, fs = [], []
    for i, p in enumerate(edge):
        vs += [p, (p[0] * 0.6, 0.95, p[2] - 0.25)]
        if i > 0:
            a = (i - 1) * 2
            fs.append((a, a + 2, a + 3, a + 1))
    mesh_obj("ciel", vs, fs, "Metal", bevel=0.0, smooth=True)
    bow = spline([(s * 0.62, 0.70, -0.10), (s * 0.50, 0.50, 0.35), (s * 0.24, 0.22, 0.66), (s * 0.04, 0.08, 0.80)], 8)
    tube("arceau", bow, 0.034, "Metal")
    for k in range(2, len(bow) - 2, 3):
        p = Vector(bow[k])
        sphere("rivet", tuple(p - p.normalized() * 0.034), 0.006, "Chrome")

# ---------- Appuis de fenêtres rembourrés, parois nervurées ----------
for s in (-1.0, 1.0):
    rail = spline([(s * 1.0, -0.26, -0.3), (s * 0.72, -0.24, 0.25), (s * 0.40, -0.20, 0.62)], 8)
    tube("appui", rail, 0.055, "Black")
    vs, fs = [], []
    for i, p in enumerate(rail):
        q = (p[0] - s * 0.05, p[1] - 0.04, p[2])
        vs += [q, (q[0], -1.0, q[2])]
        if i > 0:
            a = (i - 1) * 2
            fs.append((a, a + 2, a + 3, a + 1))
    mesh_obj("paroi", vs, fs, "Ribs", bevel=0.0, smooth=True)
    # Lattes horizontales le long de la paroi, sous l'appui
    for h in (0.10, 0.20, 0.30, 0.40, 0.52):
        latte = [(p[0] - s * 0.055, p[1] - 0.04 - h, p[2]) for p in rail]
        tube("latte", latte, 0.010, "Metal")

# ---------- Consoles latérales : caissons, panneaux, interrupteurs, légendes éclairées ----------
for s in (-1.0, 1.0):
    xi, xo = s * 0.42, s * 0.78
    lo, hi = (min(xi, xo), max(xi, xo))
    box("caisson", (lo, -1.0, -0.25), (hi, -0.64, 0.48), "Console", bevel=0.012, seg=3)
    for row in range(4):
        for col in range(4):
            x = s * (0.48 + col * 0.07)
            z = -0.15 + row * 0.13
            y = -0.64
            box("embase", (x - 0.014, y, z - 0.014), (x + 0.014, y + 0.01, z + 0.014), "Black", bevel=0.003)
            tube("bascule", [(x, y + 0.01, z), (x, y + 0.025, z - 0.006), (x, y + 0.042, z - 0.012)], 0.0035, "Chrome")
            box("voyant", (x - 0.016, y + 0.001, z + 0.022), (x + 0.016, y + 0.004, z + 0.027), "E_Green", bevel=0.0)
    # Cadrans ronds de la console droite, grille de ventilation
    if s > 0:
        for k in range(3):
            cyl("cadran_console", (0.50 + k * 0.08, -0.63, 0.40), (0, 1, 0), 0.03, 0.012, "Metal", verts=24)
            cyl("verre_console", (0.50 + k * 0.08, -0.623, 0.40), (0, 1, 0), 0.025, 0.002, "Glass", verts=24, bevel=0.0)

# Quadrant de manette (gauche) : socle à fente ; les leviers sont des objets à part (animés par le jeu)
box("quadrant", (-0.67, -0.66, -0.20), (-0.53, -0.60, 0.32), "Black", bevel=0.008, seg=3)
for k, dx in enumerate((-0.035, 0.035)):
    x = -0.60 + dx
    lev = [tube("levier", [(x, -0.62, -0.20), (x, -0.56, -0.17), (x, -0.52, -0.16)], 0.009, "Chrome"),
           box("poignee", (x - 0.022, -0.535, -0.185), (x + 0.022, -0.495, -0.135), "Metal", bevel=0.008, seg=3)]
    for o in lev:
        o["throttle"] = 1
        o.name = "throttle_%d" % k

# ---------- Plancher, siège, poignée d'éjection, manche ----------
mesh_obj("plancher", [(-0.9, -1.0, -0.3), (0.9, -1.0, -0.3), (0.9, -1.0, 0.9), (-0.9, -1.0, 0.9)], [(0, 1, 2, 3)], "Floor", bevel=0.0)
for s in (-1.0, 1.0):
    tube("bourrelet", spline([(s * 0.30, -1.05, -0.10), (s * 0.26, -0.95, 0.10), (s * 0.18, -0.98, 0.22)], 6), 0.07, "Black")
# Poignée d'éjection annelée jaune / noire
for k in range(12):
    torus("ejection", (0.0, -0.93, 0.22), (0, 1, 0.25), 0.08, 0.013, "Yellow" if k % 2 == 0 else "Black", seg=4, arc=(k / 12, (k + 1) / 12))
# Manche : soufflet annelé, fût, poignée, bouton rouge, gâchette
for k in range(7):
    torus("soufflet", (0.0, -0.99 + k * 0.016, 0.365), (0, 1, 0), 0.06 - k * 0.004, 0.009, "Yellow" if k % 2 == 0 else "Black", seg=24)
cyl("fut", (0.0, -0.75, 0.378), (0, 1, 0.05), 0.016, 0.30, "Metal", verts=16)
grip = box("poignee_manche", (-0.032, -0.60, 0.36), (0.032, -0.46, 0.42), "Black", bevel=0.02, seg=4)
cyl("bouton_manche", (0.0, -0.455, 0.405), (0, 1, 0.2), 0.009, 0.01, "Red", verts=16)
box("gachette", (-0.008, -0.56, 0.418), (0.008, -0.53, 0.43), "Metal", bevel=0.003)

# ---------- Conversion en maillages, UV, occlusion ambiante cuite ----------
bpy.ops.object.select_all(action="DESELECT")
for o in list(scene.objects):
    o.select_set(True)
bpy.context.view_layer.objects.active = [o for o in scene.objects if o.type == "MESH"][0]
bpy.ops.object.convert(target="MESH")

# Regroupe tout sauf les leviers (objets animés)
static = [o for o in scene.objects if o.type == "MESH" and not o.name.startswith("throttle")]
bpy.ops.object.select_all(action="DESELECT")
for o in static:
    o.select_set(True)
bpy.context.view_layer.objects.active = static[0]
bpy.ops.object.join()
cockpit = bpy.context.active_object
cockpit.name = "cockpit"
for k in (0, 1):
    parts = [o for o in scene.objects if o.type == "MESH" and o.name.startswith("throttle_%d" % k)]
    bpy.ops.object.select_all(action="DESELECT")
    for o in parts:
        o.select_set(True)
    bpy.context.view_layer.objects.active = parts[0]
    bpy.ops.object.join()
    bpy.context.active_object.name = "throttle_%d" % k

meshes = [o for o in scene.objects if o.type == "MESH"]
for o in meshes:
    bpy.ops.object.select_all(action="DESELECT")
    o.select_set(True)
    bpy.context.view_layer.objects.active = o
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(island_margin=0.02)
    bpy.ops.object.mode_set(mode="OBJECT")
    o.data.color_attributes.new("AO", "BYTE_COLOR", "POINT")
    o.data.color_attributes.active_color = o.data.color_attributes["AO"]

scene.render.engine = "CYCLES"
scene.cycles.device = "CPU"
scene.cycles.samples = 48
scene.render.bake.target = "VERTEX_COLORS"
# Monde clair : l'occlusion n'a pas besoin de lumières
world = bpy.data.worlds.new("w")
scene.world = world
bpy.ops.object.select_all(action="DESELECT")
for o in meshes:
    o.select_set(True)
bpy.context.view_layer.objects.active = meshes[0]
bpy.ops.object.bake(type="AO")

bpy.ops.wm.obj_export(
    filepath=os.path.join(OUT, "cockpit.obj"),
    export_selected_objects=False,
    export_uv=True,
    export_normals=True,
    export_colors=True,
    export_materials=True,
    export_triangulated_mesh=True,
    apply_modifiers=True,
    forward_axis="Y",
    up_axis="Z",
)
tri = sum(len(o.data.polygons) for o in meshes)
print("COCKPIT EXPORTE :", len(meshes), "objets,", tri, "faces")
