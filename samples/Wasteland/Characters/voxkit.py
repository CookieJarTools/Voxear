"""Building blocks for the soldiers: cubes in bones, materials with shades, posing and writing `.boxmodel` files.

A model is made of bones, each a grid of cubes in the bone's own space, one cube a pixel of the model. Shapes are
added and painted with the helpers here, then `write_model` keeps it as a `.boxmodel`: each bone one sculpted box
of cubes, the palette shared by all of them, and the clips keyed from poses worked out in the model's space.
"""

import base64
import math
import struct

import numpy as np


# Materials and their shades -------------------------------------------------------------------------------------


def hex_rgb(text):
    text = text.lstrip("#")
    return tuple(int(text[at : at + 2], 16) for at in (0, 2, 4))


def hashed(*values):
    """A number from 0 to 1 that is always the same for the same whole numbers."""
    h = 2166136261
    for value in values:
        h = ((h ^ (int(value) & 0xFFFFFFFF)) * 16777619) & 0xFFFFFFFF
    h ^= h >> 15
    h = (h * 2246822519) & 0xFFFFFFFF
    h ^= h >> 13
    return h / 0xFFFFFFFF


class Palette:
    """Materials, each a base colour and a few shades of it the cubes pick between."""

    SHADES = (0.82, 0.91, 1.0, 1.08)

    def __init__(self):
        self.materials = {}
        self.colors = []

    def add(self, name, color, shades=SHADES, warm=0.0):
        r, g, b = hex_rgb(color)
        made = []
        for at, shade in enumerate(shades):
            # Darker shades lean a touch cooler and lighter ones warmer, as worn cloth and metal do.
            tilt = (at - (len(shades) - 1) / 2) * warm
            rgb = (r * shade * (1 + tilt), g * shade, b * shade * (1 - tilt))
            rgb = tuple(max(0, min(255, round(c))) for c in rgb)
            if rgb not in self.colors:
                self.colors.append(rgb)
            made.append(self.colors.index(rgb) + 1)
        self.materials[name] = made
        assert len(self.colors) <= 255, "a model's palette holds 255 colours"

    def cell(self, name, shade):
        shades = self.materials[name]
        return shades[max(0, min(len(shades) - 1, shade))]


# Cubes in a bone -----------------------------------------------------------------------------------------------


class Grid:
    """A bone's cubes: a material and a shade at each whole-numbered place, the cube from it to it plus one.

    Shapes are tested on every cube in their bounds at once: a shape's `inside` takes arrays of the cubes' middles
    and gives which are in it, so write it with `&`, `|` and `~` rather than `and`, `or` and `not`."""

    def __init__(self, seed=0, patch=3):
        self.cells = {}
        self.seed = seed
        self.patch = patch
        # With panels, each shape takes one shade for all its cubes, a few cubes picking another, so a model reads
        # as plates, pouches and straps of their own tones rather than as speckled cubes.
        self.panels = False
        self.shapes = 0

    def put(self, cell, material, shade=None):
        x, y, z = (int(math.floor(v)) for v in cell)
        cell = (x, y, z)
        if shade is None:
            shade = self.shade_at(cell)
        self.cells[(x, y, z)] = (material, shade)

    def shade_at(self, cell):
        x, y, z = cell
        # Patches a few cubes across, then a little of each cube's own, so cloth reads as worn and not as noise.
        p = self.patch
        patch = hashed(x // p, y // p, z // p, self.seed, 7)
        own = hashed(x, y, z, self.seed)
        value = patch * 0.6 + own * 0.4
        return 0 if value < 0.2 else 1 if value < 0.45 else 2 if value < 0.85 else 3

    def material(self, cell):
        there = self.cells.get(tuple(cell))
        return there[0] if there else None

    def _cells(self, inside, bounds):
        (x0, y0, z0), (x1, y1, z1) = bounds
        xs = np.arange(math.floor(x0), math.ceil(x1))
        ys = np.arange(math.floor(y0), math.ceil(y1))
        zs = np.arange(math.floor(z0), math.ceil(z1))
        if not (len(xs) and len(ys) and len(zs)):
            return []
        x, y, z = np.meshgrid(xs, ys, zs, indexing="ij")
        hit = np.broadcast_to(inside(x + 0.5, y + 0.5, z + 0.5), x.shape)
        return zip(x[hit].tolist(), y[hit].tolist(), z[hit].tolist())

    def fill(self, inside, bounds, material, shade=None, only=None):
        """Puts `material` in every cube in `bounds` whose middle `inside` takes; with `only`, just over cubes
        already there of those materials (or any, for True)."""
        self.shapes += 1
        panel = shade
        if shade is None and self.panels:
            panel = int(hashed(self.seed, self.shapes, 3) * 3.99)
        for cell in self._cells(inside, bounds):
            if panel is not None and shade is None:
                pick = hashed(*cell, self.seed, 11)
                shade_here = panel if pick > 0.12 else max(0, min(3, panel + (1 if pick > 0.06 else -1)))
            else:
                shade_here = shade
            if only is not None:
                there = self.cells.get(cell)
                if there is None or (only is not True and there[0] not in only):
                    continue
            self.put(cell, material, shade_here)

    def remove(self, inside, bounds):
        for cell in self._cells(inside, bounds):
            self.cells.pop(cell, None)

    def box(self, lo, hi, material, shade=None, round_edges=0, only=None):
        """A box from `lo` to `hi`, its edges cut back `round_edges` cubes."""
        def inside(x, y, z):
            if round_edges <= 0:
                return np.ones_like(x, bool)
            out = sum(((v < a + round_edges) | (v > b - round_edges)).astype(int)
                      for v, a, b in ((x, lo[0], hi[0]), (y, lo[1], hi[1]), (z, lo[2], hi[2])))
            return out < 2
        self.fill(inside, (lo, hi), material, shade, only)

    def block(self, middle, half, material, chamfer=1.0, shade=None, only=None, bevel=True):
        """A straight-sided block about `middle`, its corners cut back `chamfer` cubes, and with `bevel` the cubes
        along its edges a shade lighter, as a softened edge catches the light."""
        lo = tuple(m - h for m, h in zip(middle, half))
        hi = tuple(m + h for m, h in zip(middle, half))
        self.box(lo, hi, material, shade=shade, round_edges=chamfer, only=only)
        if not bevel:
            return
        for (x, y, z), (there, tone) in list(self.cells.items()):
            if there != material:
                continue
            near = sum(1 for v, a, b in ((x + 0.5, lo[0], hi[0]), (y + 0.5, lo[1], hi[1]), (z + 0.5, lo[2], hi[2]))
                       if a <= v <= b and (v - a < chamfer + 1.0 or b - v < chamfer + 1.0))
            inside = all(a <= v <= b for v, a, b in ((x + 0.5, lo[0], hi[0]), (y + 0.5, lo[1], hi[1]),
                                                     (z + 0.5, lo[2], hi[2])))
            if inside and near >= 2:
                self.cells[(x, y, z)] = (there, min(3, tone + 1))

    def bevelled(self, middle, half, material, cut=6.0, shade=None, only=None):
        """A block whose edges are cut back on a slope `cut` cubes deep, so it reads as a rounded box of chunky
        steps while its faces stay flat."""
        lo = np.array(middle, float) - half
        hi = np.array(middle, float) + half

        def inside(x, y, z):
            e = [np.minimum(v - a, b - v) for v, a, b in ((x, lo[0], hi[0]), (y, lo[1], hi[1]), (z, lo[2], hi[2]))]
            ok = (e[0] >= 0) & (e[1] >= 0) & (e[2] >= 0)
            return ok & (e[0] + e[1] >= cut) & (e[1] + e[2] >= cut) & (e[0] + e[2] >= cut) & \
                (e[0] + e[1] + e[2] >= cut * 1.5)

        self.fill(inside, (lo, hi), material, shade, only)

    def rounded(self, middle, half, material, power=3.0, shade=None, only=None, keep=None):
        """A box with its corners worn round: |x|^p + |y|^p + |z|^p <= 1 over its half sizes."""
        cx, cy, cz = middle
        hx, hy, hz = half

        def inside(x, y, z):
            made = (np.abs(x - cx) / hx) ** power + (np.abs(y - cy) / hy) ** power + (np.abs(z - cz) / hz) ** power <= 1
            return made & keep(x, y, z) if keep is not None else made

        self.fill(inside, ((cx - hx, cy - hy, cz - hz), (cx + hx, cy + hy, cz + hz)), material, shade, only)

    def ball(self, middle, radii, material, shade=None, only=None, keep=None):
        self.rounded(middle, radii, material, 2.0, shade, only, keep)

    def rod(self, start, end, radius, material, shade=None, only=None, square=False, flat=False, radius2=None):
        """A rod between two points, round or square across, its ends rounded or, with `flat`, cut flat; with
        `radius2`, narrowing or widening to it at the end."""
        a = np.array(start, float)
        b = np.array(end, float)
        along = b - a
        length2 = max(float(along @ along), 1e-9)
        r2 = radius if radius2 is None else radius2

        def inside(x, y, z):
            px, py, pz = x - a[0], y - a[1], z - a[2]
            t = (px * along[0] + py * along[1] + pz * along[2]) / length2
            ok = (t >= 0) & (t <= 1) if flat else np.ones_like(t, bool)
            t = np.clip(t, 0, 1)
            ox, oy, oz = px - along[0] * t, py - along[1] * t, pz - along[2] * t
            r = radius + (r2 - radius) * t
            if square:
                return ok & (np.maximum(np.maximum(np.abs(ox), np.abs(oy)), np.abs(oz)) <= r)
            return ok & (ox * ox + oy * oy + oz * oz <= r * r)

        reach_ = max(radius, r2) + 1
        self.fill(inside, (np.minimum(a, b) - reach_, np.maximum(a, b) + reach_), material, shade, only)

    def path(self, points, radius, material, shade=None, only=None, square=False):
        """Rods from point to point."""
        for a, b in zip(points, points[1:]):
            self.rod(a, b, radius, material, shade, only, square)

    def shell(self, material, out=1, only=None, keep=None):
        """Cubes of `material` over every face of what is there, `out` deep, where `keep` takes them: a coat over
        a body, a wrap over a limb."""
        for _ in range(out):
            around = set()
            for (x, y, z), there in self.cells.items():
                if only is not None and there[0] not in only:
                    continue
                for dx, dy, dz in ((1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)):
                    cell = (x + dx, y + dy, z + dz)
                    if cell not in self.cells:
                        around.add(cell)
            for cell in around:
                if keep is None or keep(*[c + 0.5 for c in cell]):
                    self.put(cell, material)

    def paint(self, inside, bounds, material, shade=None, only=True):
        """Colours cubes already there."""
        self.fill(inside, bounds, material, shade, only)

    def recolor(self, old, new, chance=1.0, seed=0, keep=None):
        for cell, (material, _) in list(self.cells.items()):
            if material == old and hashed(*cell, seed) < chance and (keep is None or keep(*cell)):
                self.put(cell, new)

    def surface(self, cell):
        x, y, z = cell
        return any((x + dx, y + dy, z + dz) not in self.cells
                   for dx, dy, dz in ((1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)))

    def mirrored(self):
        """The grid turned over across X, as the limb on the other side wants."""
        made = Grid(self.seed + 1, self.patch)
        for (x, y, z), value in self.cells.items():
            made.cells[(-x - 1, y, z)] = value
        return made

    def merge(self, other, offset=(0, 0, 0)):
        ox, oy, oz = offset
        for (x, y, z), value in other.cells.items():
            self.cells[(x + ox, y + oy, z + oz)] = value

    def bounds(self):
        xs, ys, zs = zip(*self.cells)
        return (min(xs), min(ys), min(zs)), (max(xs) + 1, max(ys) + 1, max(zs) + 1)


# Turning, as the engine reads it: degrees about Y, then X, then Z ------------------------------------------------


def rot_x(a):
    c, s = math.cos(a), math.sin(a)
    return np.array([[1, 0, 0], [0, c, -s], [0, s, c]])


def rot_y(a):
    c, s = math.cos(a), math.sin(a)
    return np.array([[c, 0, s], [0, 1, 0], [-s, 0, c]])


def rot_z(a):
    c, s = math.cos(a), math.sin(a)
    return np.array([[c, -s, 0], [s, c, 0], [0, 0, 1]])


def turned(degrees):
    x, y, z = (math.radians(v) for v in degrees)
    return rot_y(y) @ rot_x(x) @ rot_z(z)


def degrees_of(m):
    """The degrees about X, Y and Z that `turned` makes `m` from."""
    x = math.asin(max(-1.0, min(1.0, -m[1, 2])))
    if abs(m[1, 2]) < 0.9999:
        y = math.atan2(m[0, 2], m[2, 2])
        z = math.atan2(m[1, 0], m[1, 1])
    else:
        y = math.atan2(-m[2, 0], m[0, 0])
        z = 0.0
    return [round(math.degrees(v), 3) + 0.0 for v in (x, y, z)]


def basis(forward, up):
    """A turning whose +Z is `forward` and whose +Y is as near `up` as can be."""
    z = np.array(forward, float)
    z /= np.linalg.norm(z)
    y = np.array(up, float)
    y = y - z * (y @ z)
    y /= np.linalg.norm(y)
    x = np.cross(y, z)
    return np.column_stack((x, y, z))


# The model -----------------------------------------------------------------------------------------------------


class Bone:
    def __init__(self, name, parent=None, pivot=(0, 0, 0), rotation=(0, 0, 0), role=None, seed=0):
        self.name = name
        self.parent = parent
        self.pivot = np.array(pivot, float)
        self.rotation = list(rotation)
        self.role = role
        self.grid = Grid(seed)
        self.place = np.zeros(3)  # where its pivot is in the model at rest, before any turning


class Model:
    def __init__(self, palette, pixels_per_block):
        self.palette = palette
        self.pixels_per_block = pixels_per_block
        self.bones = []
        self.clips = []

    def bone(self, name, parent=None, pivot=(0, 0, 0), rotation=(0, 0, 0), role=None):
        made = Bone(name, parent, pivot, rotation, role, seed=len(self.bones) * 31 + 5)
        self.bones.append(made)
        return made

    def by_name(self, name):
        return next(bone for bone in self.bones if bone.name == name)

    def placed(self, keys=None):
        """Each bone's turning and place in the model's space, with `keys` (bone name: (degrees, move)) over
        its rest as a clip's keys are."""
        keys = keys or {}
        made = {}
        for bone in self.bones:
            rotation, move = keys.get(bone.name, ((0, 0, 0), (0, 0, 0)))
            # A turning given whole (a matrix) is the bone's turning as its parent holds it; degrees are from rest.
            local_r = rotation if isinstance(rotation, np.ndarray) else turned(bone.rotation) @ turned(rotation)
            local_t = bone.pivot + np.array(move, float)
            if bone.parent is None:
                made[bone.name] = (local_r, local_t)
            else:
                pr, pt = made[bone.parent]
                made[bone.name] = (pr @ local_r, pt + pr @ local_t)
        return made

    def key_for(self, name, world_r, world_t, placed):
        """The key that puts bone `name` turned `world_r` at `world_t` in the model's space, its parent where
        `placed` has it."""
        bone = self.by_name(name)
        pr, pt = placed[bone.parent]
        local_r = pr.T @ world_r
        local_t = pr.T @ (np.array(world_t, float) - pt)
        key_r = turned(bone.rotation).T @ local_r
        return degrees_of(key_r), list(local_t - bone.pivot)


# Reaching: an arm of two bones brought exactly to a hand's place and turning ---------------------------------------


def two_bone(model, upper, lower, hand, wrist_r, wrist_t, pole, placed):
    """Turns for `upper`, `lower` and `hand` (bones built hanging down their -Y) that put the hand turned `wrist_r`
    with its pivot at `wrist_t` in the model's space, the elbow bending towards `pole`. The lower bone bends about
    its X only, then twists along itself as far as takes the most turn off the wrist. Gives each bone's turning
    as its parent holds it, and how far short the hand falls."""
    up_bone, low_bone = model.by_name(upper), model.by_name(lower)
    hand_bone = model.by_name(hand)
    pr, pt = placed[up_bone.parent]
    shoulder = pt + pr @ up_bone.pivot
    l1 = float(np.linalg.norm(low_bone.pivot))
    l2 = float(np.linalg.norm(hand_bone.pivot))
    to = np.array(wrist_t, float) - shoulder
    d = float(np.linalg.norm(to))
    short = max(0.0, d - (l1 + l2 - 0.02))
    d = min(max(d, abs(l1 - l2) + 0.02), l1 + l2 - 0.02)
    u = to / max(np.linalg.norm(to), 1e-9)
    a = (l1 * l1 - l2 * l2 + d * d) / (2 * d)
    h = math.sqrt(max(0.0, l1 * l1 - a * a))
    p = np.array(pole, float)
    v = p - u * (p @ u)
    v /= max(np.linalg.norm(v), 1e-9)
    elbow = shoulder + u * a + v * h
    wrist = shoulder + u * d
    # The upper arm: its -Y down to the elbow, its +Z towards where the forearm goes, so the elbow bends about X.
    y_axis = -(elbow - shoulder) / l1
    fore = wrist - elbow
    z_axis = fore - y_axis * (fore @ y_axis)
    z_axis /= max(np.linalg.norm(z_axis), 1e-9)
    upper_w = np.column_stack((np.cross(y_axis, z_axis), y_axis, z_axis))
    # The bone's own pivot offset to the elbow is along its -Y at rest; turn so it lands on the elbow.
    rest_dir = -low_bone.pivot / l1
    upper_w = upper_w @ align(np.array((0.0, -1.0, 0.0)), -rest_dir)
    bend = math.acos(max(-1.0, min(1.0, float((-y_axis) @ (fore / np.linalg.norm(fore))))))
    best = None
    wrist_r = np.array(wrist_r, float)
    for twist in np.linspace(-110, 110, 221):
        lower_w = upper_w @ rot_x(-bend) @ rot_y(math.radians(twist))
        rel = lower_w.T @ wrist_r
        angle = math.acos(max(-1.0, min(1.0, (np.trace(rel) - 1) / 2)))
        if best is None or angle < best[0]:
            best = (angle, lower_w)
    lower_w = best[1]
    return {
        upper: pr.T @ upper_w,
        lower: upper_w.T @ lower_w,
        hand: lower_w.T @ wrist_r,
    }, short


def align(a, b):
    """The turning that takes direction `a` to direction `b`."""
    a = a / np.linalg.norm(a)
    b = b / np.linalg.norm(b)
    v = np.cross(a, b)
    c = float(a @ b)
    if np.linalg.norm(v) < 1e-9:
        return np.eye(3) if c > 0 else rot_z(math.pi)
    k = np.array([[0, -v[2], v[1]], [v[2], 0, -v[0]], [-v[1], v[0], 0]])
    return np.eye(3) + k + k @ k * (1 / (1 + c))


# Writing -------------------------------------------------------------------------------------------------------


def lz4_block(data):
    """Compresses `data` as an LZ4 block with its size before it, as the engine reads sculpted boxes."""
    out = bytearray(struct.pack("<I", len(data)))
    n = len(data)
    table = {}
    anchor = 0
    at = 0
    limit = n - 12

    def length_bytes(value):
        made = bytearray()
        while value >= 255:
            made.append(255)
            value -= 255
        made.append(value)
        return made

    while at < limit:
        key = bytes(data[at : at + 4])
        found = table.get(key)
        table[key] = at
        if found is None or at - found > 65535:
            at += 1
            continue
        length = 4
        while at + length < n - 5 and data[found + length] == data[at + length]:
            length += 1
        literals = data[anchor:at]
        token_lit = min(len(literals), 15)
        token_match = min(length - 4, 15)
        out.append((token_lit << 4) | token_match)
        if len(literals) >= 15:
            out += length_bytes(len(literals) - 15)
        out += literals
        out += struct.pack("<H", at - found)
        if length - 4 >= 15:
            out += length_bytes(length - 4 - 15)
        at += length
        anchor = at
    literals = data[anchor:]
    out.append(min(len(literals), 15) << 4)
    if len(literals) >= 15:
        out += length_bytes(len(literals) - 15)
    out += literals
    return bytes(out)


def fmt(value):
    value = round(float(value), 3) + 0.0
    return repr(value) if value != int(value) else f"{int(value)}.0"


def vec(values):
    return "[" + ", ".join(fmt(v) for v in values) + "]"


def write_model(model, path, tints=None):
    palette = model.palette
    lines = [
        "# Voxear box model: boxes under bones, measured in the pixels of its sheet.",
        f"pixels_per_block = {fmt(model.pixels_per_block)}",
        "sheet = [16, 16]",
        "palette = [" + ", ".join(f'"{r:02x}{g:02x}{b:02x}"' for r, g, b in palette.colors) + "]",
        "",
    ]
    if tints:
        lines.append("[tints]")
        for color, slot in tints.items():
            lines.append(f'{color} = "{slot}"')
        lines.append("")
    for bone in model.bones:
        lines.append("[[bones]]")
        lines.append(f'name = "{bone.name}"')
        if bone.role:
            lines.append(f'role = "{bone.role}"')
        if bone.parent:
            lines.append(f'parent = "{bone.parent}"')
        lines.append(f"pivot = {vec(bone.pivot)}")
        if any(bone.rotation):
            lines.append(f"rotation = {vec(bone.rotation)}")
        lines.append("")
        if not bone.grid.cells:
            continue
        lo, hi = bone.grid.bounds()
        size = [hi[a] - lo[a] for a in range(3)]
        assert all(1 <= s <= 256 for s in size), f"{bone.name} is {size} cubes"
        cells = bytearray(size[0] * size[1] * size[2])
        for (x, y, z), (material, shade) in bone.grid.cells.items():
            at = (x - lo[0]) + size[0] * ((y - lo[1]) + size[1] * (z - lo[2]))
            cells[at] = palette.cell(material, shade)
        lines.append("[[bones.boxes]]")
        lines.append(f"from = {vec(lo)}")
        lines.append(f"size = [{size[0]}, {size[1]}, {size[2]}]")
        lines.append(f'voxels = "{base64.b64encode(lz4_block(bytes(cells))).decode()}"')
        lines.append("")
    for clip in model.clips:
        lines.append("[[clips]]")
        lines.append(f'name = "{clip["name"]}"')
        lines.append(f"length = {fmt(clip['length'])}")
        if clip.get("looping"):
            lines.append("looping = true")
        lines.append("")
        for bone, keys in clip["bones"].items():
            lines.append("[[clips.bones]]")
            lines.append(f'bone = "{bone}"')
            lines.append("times = [" + ", ".join(fmt(t) for t, _, _ in keys) + "]")
            lines.append("rotation = [" + ", ".join(vec(r) for _, r, _ in keys) + "]")
            if any(any(abs(v) > 1e-3 for v in m) for _, _, m in keys):
                lines.append("move = [" + ", ".join(vec(m) for _, _, m in keys) + "]")
            lines.append('ease = [' + ", ".join('"smooth"' for _ in keys) + "]")
            lines.append("")
    with open(path, "w") as out:
        out.write("\n".join(lines))


# Checking a pose --------------------------------------------------------------------------------------------------


def cells_placed(model, keys, names):
    """The middles of the named bones' cubes in the model's space, posed by `keys`, as whole-number places."""
    placed = model.placed(keys)
    made = set()
    for name in names:
        bone = model.by_name(name)
        if not bone.grid.cells:
            continue
        r, t = placed[name]
        middles = np.array(list(bone.grid.cells.keys()), float) + 0.5
        for p in np.floor(middles @ r.T + t).astype(int):
            made.add(tuple(p))
    return made


def clipping(model, keys, moving, still):
    """How many cubes of the `moving` bones lie inside the `still` ones in a pose."""
    return len(cells_placed(model, keys, moving) & cells_placed(model, keys, still))
