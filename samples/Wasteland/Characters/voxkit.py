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
    """A bone's cubes: a material and a shade at each whole-numbered place, the cube from it to it plus one."""

    def __init__(self, seed=0):
        self.cells = {}
        self.seed = seed

    def put(self, cell, material, shade=None):
        x, y, z = cell
        if shade is None:
            shade = self.shade_at(cell)
        self.cells[(x, y, z)] = (material, shade)

    def shade_at(self, cell):
        x, y, z = cell
        # Patches two cubes across, then a little of each cube's own, so cloth reads as worn and not as noise.
        patch = hashed(x // 2, y // 2, z // 2, self.seed, 7)
        own = hashed(x, y, z, self.seed)
        value = patch * 0.65 + own * 0.35
        return 0 if value < 0.2 else 1 if value < 0.45 else 2 if value < 0.85 else 3

    def fill(self, inside, bounds, material, shade=None, only=None):
        """Puts `material` in every cube in `bounds` whose middle `inside` takes; with `only`, just over cubes
        already there of those materials (or any, for True)."""
        (x0, y0, z0), (x1, y1, z1) = bounds
        for x in range(math.floor(x0), math.ceil(x1)):
            for y in range(math.floor(y0), math.ceil(y1)):
                for z in range(math.floor(z0), math.ceil(z1)):
                    if not inside(x + 0.5, y + 0.5, z + 0.5):
                        continue
                    if only is not None:
                        there = self.cells.get((x, y, z))
                        if there is None or (only is not True and there[0] not in only):
                            continue
                    self.put((x, y, z), material, shade)

    def box(self, lo, hi, material, shade=None, round_edges=0, only=None):
        """A box from `lo` to `hi`, its edges cut back `round_edges` cubes."""
        def inside(x, y, z):
            if round_edges <= 0:
                return True
            out = 0
            for v, a, b in ((x, lo[0], hi[0]), (y, lo[1], hi[1]), (z, lo[2], hi[2])):
                if v < a + round_edges or v > b - round_edges:
                    out += 1
            return out < 2
        self.fill(inside, (lo, hi), material, shade, only)

    def ball(self, middle, radii, material, shade=None, only=None, keep=None):
        cx, cy, cz = middle
        rx, ry, rz = radii

        def inside(x, y, z):
            if keep is not None and not keep(x, y, z):
                return False
            return ((x - cx) / rx) ** 2 + ((y - cy) / ry) ** 2 + ((z - cz) / rz) ** 2 <= 1.0

        self.fill(inside, ((cx - rx, cy - ry, cz - rz), (cx + rx, cy + ry, cz + rz)), material, shade, only)

    def rod(self, start, end, radius, material, shade=None, only=None, square=False, flat=False):
        """A rod between two points, round or square across, its ends rounded or, with `flat`, cut flat."""
        a = np.array(start, float)
        b = np.array(end, float)
        along = b - a
        length2 = max(float(along @ along), 1e-9)

        def inside(x, y, z):
            p = np.array((x, y, z))
            t = float((p - a) @ along) / length2
            if flat and not 0.0 <= t <= 1.0:
                return False
            t = max(0.0, min(1.0, t))
            off = p - (a + along * t)
            if square:
                return max(abs(off)) <= radius
            return float(off @ off) <= radius * radius

        lo = np.minimum(a, b) - radius - 1
        hi = np.maximum(a, b) + radius + 1
        self.fill(inside, (lo, hi), material, shade, only)

    def remove(self, inside, bounds):
        (x0, y0, z0), (x1, y1, z1) = bounds
        for x in range(math.floor(x0), math.ceil(x1)):
            for y in range(math.floor(y0), math.ceil(y1)):
                for z in range(math.floor(z0), math.ceil(z1)):
                    if inside(x + 0.5, y + 0.5, z + 0.5):
                        self.cells.pop((x, y, z), None)

    def mirrored(self):
        """The grid turned over across X, as the limb on the other side wants."""
        made = Grid(self.seed + 1)
        for (x, y, z), value in self.cells.items():
            made.cells[(-x - 1, y, z)] = value
        return made

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
            local_r = turned(bone.rotation) @ turned(rotation)
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


# Reaching: an arm of two bones brought to a point by turning them -------------------------------------------------


def reach(model, upper, lower, hand, target, placed, keys, elbow_out, start=None):
    """Keys for `upper` and `lower` (an elbow turning about its X) that put the point `hand` of the lower bone at
    `target`, the elbow kept out to the side `elbow_out` points. Searched for, as there are few bones."""
    target = np.array(target, float)
    best = None

    def cost(values):
        trial = dict(keys)
        trial[upper] = ((values[0], values[1], values[2]), (0, 0, 0))
        trial[lower] = ((values[3], 0, 0), (0, 0, 0))
        at = model.placed(trial)
        r, t = at[lower]
        point = t + r @ np.array(hand, float)
        er, et = at[lower]
        miss = float(np.sum((point - target) ** 2))
        # Elbows out and down rather than up into the head; a bend that stays a bend.
        side = float(np.array(elbow_out) @ (et - at[upper][1]))
        return miss - 0.05 * side + 0.002 * max(0.0, values[3]) ** 2

    values = np.array(start if start is not None else [-40.0, 0.0, 0.0, -60.0])
    step = 24.0
    current = cost(values)
    while step > 0.05:
        improved = False
        for axis in range(4):
            for sign in (1, -1):
                trial = values.copy()
                trial[axis] += sign * step
                c = cost(trial)
                if c < current:
                    values, current, improved = trial, c, True
        if not improved:
            step *= 0.5
    best = values
    keys = dict(keys)
    keys[upper] = ((round(best[0], 2), round(best[1], 2), round(best[2], 2)), (0, 0, 0))
    keys[lower] = ((round(best[3], 2), 0, 0), (0, 0, 0))
    return keys, math.sqrt(max(0.0, current))


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


# A quick look without the engine --------------------------------------------------------------------------------


def preview(model, path, keys=None, size=640, yaw=35.0, pitch=20.0, scale=None):
    """Draws the model's cubes as a picture, lit from above and the front left, to look at while making it."""
    from PIL import Image, ImageDraw

    placed = model.placed(keys)
    view = rot_x(math.radians(pitch)) @ rot_y(math.radians(-yaw))
    light = np.array([-0.5, 0.8, 0.6])
    light /= np.linalg.norm(light)
    faces = []
    corners = {
        (1, 0, 0): [(1, 0, 0), (1, 1, 0), (1, 1, 1), (1, 0, 1)],
        (-1, 0, 0): [(0, 0, 0), (0, 0, 1), (0, 1, 1), (0, 1, 0)],
        (0, 1, 0): [(0, 1, 0), (0, 1, 1), (1, 1, 1), (1, 1, 0)],
        (0, -1, 0): [(0, 0, 0), (1, 0, 0), (1, 0, 1), (0, 0, 1)],
        (0, 0, 1): [(0, 0, 1), (1, 0, 1), (1, 1, 1), (0, 1, 1)],
        (0, 0, -1): [(0, 0, 0), (0, 1, 0), (1, 1, 0), (1, 0, 0)],
    }
    for bone in model.bones:
        r, t = placed[bone.name]
        cells = bone.grid.cells
        for (x, y, z), (material, shade) in cells.items():
            color = np.array(model.palette.colors[model.palette.cell(material, shade) - 1], float)
            for normal, quad in corners.items():
                if (x + normal[0], y + normal[1], z + normal[2]) in cells:
                    continue
                n = view @ (r @ np.array(normal, float))
                if n[2] <= 0:
                    continue
                points = [view @ (t + r @ np.array((x + a, y + b, z + c), float)) for a, b, c in quad]
                lit = 0.45 + 0.6 * max(0.0, float((r @ np.array(normal, float)) @ light))
                depth = sum(p[2] for p in points) / 4
                faces.append((depth, points, tuple(int(min(255, c * lit)) for c in color)))
    faces.sort(key=lambda face: face[0])
    xs = [p[0] for _, pts, _ in faces for p in pts]
    ys = [p[1] for _, pts, _ in faces for p in pts]
    if scale is None:
        scale = 0.9 * size / max(max(xs) - min(xs), max(ys) - min(ys))
    mid = ((max(xs) + min(xs)) / 2, (max(ys) + min(ys)) / 2)
    image = Image.new("RGB", (size, size), (236, 232, 226))
    draw = ImageDraw.Draw(image)
    for _, points, color in faces:
        draw.polygon(
            [(size / 2 + (p[0] - mid[0]) * scale, size / 2 - (p[1] - mid[1]) * scale) for p in points], fill=color
        )
    image.save(path)
    return scale
