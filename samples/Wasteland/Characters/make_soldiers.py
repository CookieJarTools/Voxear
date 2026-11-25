#!/usr/bin/env python3
"""Makes the Wasteland soldiers as box models of small cubes, with a humanoid's bones and their aim clips.

    python3 make_soldiers.py    # writes Rifleman.boxmodel and Scavenger.boxmodel beside this file

Each soldier is a chibi figure about 64 cubes tall at 34 to a metre (1.9 m): a big head with large eyes under a helmet
with goggles, a mask over the lower face, a short body packed with webbing and pouches, stubby arms and legs, knee
pads and chunky boots. The bones are named as a humanoid rig names them (Hips, Spine, Chest, Neck, Head,
LeftUpperArm, LeftForearm, LeftHand, LeftThigh, LeftShin, LeftFoot and the right side's), so the engine finds its
arms, legs and spine.

The hands are sculpted closed round the parts of the gun they hold, and every pose puts each hand exactly there:
the arm reaches its wrist to the place and turning the grip asks for, the elbow bending out and the forearm
twisting to spare the wrist. At rest the gun is held low across the body, so Mob Motion walks, runs and idles the
body and legs while the hands stay on it. Aiming is a clip in each model ("Aim"), with "Fire" for the kick of a
shot. Running this again makes the models afresh, losing changes made in the Model window.
"""

import math
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from voxkit import (  # noqa: E402
    Model,
    Palette,
    basis,
    clipping,
    degrees_of,
    hashed,
    rot_x,
    rot_y,
    turned,
    two_bone,
    write_model,
)

HERE = os.path.dirname(os.path.abspath(__file__))
PIXELS = 34.0  # cubes to a metre: about 1.9 m tall

# Where the bones stand, in cubes: the hips from the ground, each other from the one above.
HIPS = 18
SPINE = 5
CHEST = 6
NECK = 6
HEAD = 1
SHOULDER = (4.0, 4.8)
UPPER_ARM = 6.5
ELBOW = 10.0
WRIST = 10.0
THIGH = (4.3, -1.0)
KNEE = 7.5
ANKLE = 6.0

# A hand closed round a grip, in the hand's own space: the wrist at its pivot, the fingers reaching down -Y, the
# palm facing -X (a right hand; the left is it turned over), the thumb and index finger to +Z. The grip it holds
# runs along Z through `grip_at`.
GRIP_RADIUS = 1.5


def grip_at(radius):
    """Where a hand's grip runs, for a grip `radius` thick: just off the palm, under the knuckles."""
    return np.array((-1.2 - radius, -4.9, 0.0))


def palette(kind):
    p = Palette()
    shades = (0.78, 0.9, 1.0, 1.1)
    p.add("skin", "f0c09a", warm=0.02)
    p.add("blush", "e8a088", shades=(1.0,))
    p.add("lash", "2a1a18", shades=(1.0,))
    p.add("iris", "5a2622", shades=(1.0,))
    p.add("iris_light", "8e3e2c", shades=(1.0,))
    p.add("eye_light", "f6f0e6", shades=(1.0,))
    p.add("hair", "4a3024", shades=shades)
    p.add("leather", "7a5032", shades=shades, warm=0.04)
    p.add("leather_dark", "3e2a1e", shades=shades, warm=0.03)
    p.add("boot", "2e2826", shades=shades)
    p.add("sole", "1c1816", shades=(0.9, 1.0))
    p.add("brass", "b8924a", shades=(0.75, 0.9, 1.0, 1.15))
    p.add("steel", "4a4d52", shades=shades)
    p.add("blued", "2c2f35", shades=shades)
    p.add("wood", "8a5434", shades=shades, warm=0.05)
    p.add("tan", "b49a72", shades=shades, warm=0.03)
    p.add("tan_dark", "8a7454", shades=shades, warm=0.03)
    p.add("rag", "c2b090", shades=shades)
    p.add("rust", "8a4a2a", shades=shades, warm=0.05)
    p.add("glass", "3e5a66", shades=(0.8, 1.0))
    p.add("glint", "d6ece6", shades=(1.0,))
    if kind == "rifleman":
        p.add("cloth", "6a6846", shades=shades, warm=0.03)
        p.add("cloth_dark", "4a4934", shades=shades, warm=0.03)
        p.add("puttee", "8a7c58", shades=shades)
        p.add("helmet", "5e6046", shades=shades, warm=0.02)
        p.add("mask", "3a3e38", shades=shades)
        p.add("patch", "7a3e32", shades=shades)
        p.add("scrim", "7d7a52", shades=shades)
        p.add("glove", "5e3e28", shades=shades, warm=0.03)
    else:
        p.add("cloth", "4c5572", shades=shades, warm=-0.03)
        p.add("cloth_dark", "353b52", shades=shades, warm=-0.03)
        p.add("puttee", "5a6078", shades=shades)
        p.add("helmet", "4e5668", shades=shades, warm=-0.02)
        p.add("mask", "2e3348", shades=shades)
        p.add("patch", "8a7550", shades=shades)
        p.add("scrim", "6a6a5a", shades=shades)
        p.add("glove", "3a3430", shades=shades, warm=0.03)
    return p


def grid(model, name):
    return model.by_name(name).grid


def front_of(g, x, y):
    zs = [z for (cx, cy, z) in g.cells if cx == x and cy == y]
    return max(zs) if zs else None


def stitched(g, materials, seed, chance=0.08):
    """Darker cubes here and there on the surface: seams, dirt and wear."""
    for cell, (material, _) in list(g.cells.items()):
        if material in materials and g.surface(cell) and hashed(*cell, seed) < chance:
            g.put(cell, material, 0)


def pouch(g, lo, hi, material="tan", flap="tan_dark", stud=True):
    """A pouch: a box, its flap a darker row over the top, a stud in the middle."""
    g.box(lo, hi, material, round_edges=0)
    g.box((lo[0], hi[1] - 1.2, lo[2] - 0.01), (hi[0], hi[1], hi[2] + 0.6), flap)
    if stud:
        g.put((int(math.floor((lo[0] + hi[0]) / 2)), int(hi[1] - 2), int(math.ceil(hi[2]))), "brass", 2)


# The skeleton -----------------------------------------------------------------------------------------------------


def skeleton(model):
    model.bone("Hips", pivot=(0, HIPS, 0), role="body")
    model.bone("Spine", "Hips", (0, SPINE, 0))
    model.bone("Chest", "Spine", (0, CHEST, 0))
    model.bone("Neck", "Chest", (0, NECK, 0))
    model.bone("Head", "Neck", (0, HEAD, 0), role="head")
    for side, name in ((-1, "Left"), (1, "Right")):
        # The arms have no role: they hold the gun, and Mob Motion leaves them be.
        model.bone(f"{name}Shoulder", "Chest", (side * SHOULDER[0], SHOULDER[1], 0))
        model.bone(f"{name}UpperArm", f"{name}Shoulder", (side * UPPER_ARM, 0, 0))
        model.bone(f"{name}Forearm", f"{name}UpperArm", (0, -ELBOW, 0))
        model.bone(f"{name}Hand", f"{name}Forearm", (0, -WRIST, 0))
    for side, name in ((-1, "Left"), (1, "Right")):
        model.bone(f"{name}Thigh", "Hips", (side * THIGH[0], THIGH[1], 0), role="leg")
        model.bone(f"{name}Shin", f"{name}Thigh", (0, -KNEE, 0), role="leg")
        model.bone(f"{name}Foot", f"{name}Shin", (0, -ANKLE, 0))


# The body both soldiers share -------------------------------------------------------------------------------------


def body(model, kind):
    hips, spine, chest = grid(model, "Hips"), grid(model, "Spine"), grid(model, "Chest")
    hips.rounded((0, -1.5, 0), (8.6, 4.2, 5.6), "cloth", power=2.6)
    spine.rounded((0, 3, 0), (8.4, 4.4, 5.8), "cloth", power=2.6)
    chest.rounded((0, 3.2, 0.2), (9.2, 5.6, 6.2), "cloth", power=2.6)
    chest.rounded((0, 4.4, 0), (11.0, 2.6, 5.4), "cloth", power=2.4)  # the shoulders
    chest.rod((0, 7.5, -0.3), (0, 9.5, -0.3), 4.4, "cloth_dark", flat=True)  # a collar
    grid(model, "Neck").rod((0, -1, 0), (0, 2.5, 0.3), 3.0, "skin")
    # Arms: a padded shoulder, the sleeve, a cuff and the wrist.
    for side, name in ((-1, "Left"), (1, "Right")):
        upper = grid(model, f"{name}UpperArm")
        upper.ball((0, -1, 0), (4.0, 3.8, 4.0), "cloth")
        upper.rod((0, -1, 0), (0, -9.5, 0), 3.5, "cloth", radius2=3.2)
        upper.box((side * 2.8, -5.0, -1.5), (side * 4.1, -2.0, 1.5), "patch")  # a sleeve badge
        for dy in (0, 1.2):
            upper.rod((side * 4.0, -4.6 + dy, -1.6), (side * 4.0, -3.8 + dy, 0), 0.5, "tan", square=True)
            upper.rod((side * 4.0, -3.8 + dy, 0), (side * 4.0, -4.6 + dy, 1.6), 0.5, "tan", square=True)
        fore = grid(model, f"{name}Forearm")
        fore.ball((0, 0, 0), (3.2, 3.2, 3.2), "cloth")
        fore.rod((0, 0, 0), (0, -6.5, 0), 3.2, "cloth", radius2=3.0)
        fore.rod((0, -5.6, 0), (0, -7.6, 0), 3.4, "cloth_dark", flat=True)
        fore.rod((0, -7.6, 0), (0, -10.4, 0), 2.3, "glove", flat=True)
    # Legs: trousers, a knee pad, puttees wound to the boot, and the chunky boot.
    for side, name in ((-1, "Left"), (1, "Right")):
        thigh = grid(model, f"{name}Thigh")
        thigh.rod((0, 1.5, 0), (0, -7.5, 0), 4.2, "cloth", radius2=3.9)
        if side > 0:
            pouch(thigh, (3.0, -6.5, -2.5), (5.6, -1.5, 2.5))
            thigh.rod((0, -2, 0), (0, -3, 0), 4.5, "tan_dark", flat=True, only=None)
        shin = grid(model, f"{name}Shin")
        shin.ball((0, 0, 0), (3.9, 3.9, 3.9), "cloth")
        shin.rounded((0, -0.6, 3.0), (3.0, 3.0, 1.6), "leather", power=2.6)  # the knee pad
        shin.rod((0, -1, 0), (0, -6, 0), 3.7, "puttee", radius2=3.4)
        for (x, y, z), (material, _) in list(shin.cells.items()):
            if material == "puttee" and (y + round(math.atan2(z, x) / math.tau * 2)) % 2 == 0:
                shin.put((x, y, z), "puttee", 0)
        foot = grid(model, f"{name}Foot")
        foot.rod((0, -1.5, -0.3), (0, 2.5, -0.3), 3.9, "boot", flat=True)
        foot.rounded((0, -1.6, 1.6), (4.1, 2.6, 6.0), "boot", power=2.6)
        foot.box((-4.3, -3.5, -4.6), (4.3, -2.4, 7.8), "sole", round_edges=1)
        foot.box((-4.2, -2.4, 4.5), (4.2, 0.2, 7.8), "boot", only=("boot",), shade=3)
        for y in (-1, 0, 1, 2):
            spot = front_of(foot, 0, y)
            if spot is not None:
                foot.put((0, y, spot), "leather", 1)
                foot.put((-1, y, spot), "leather", 1)
    for g in (hips, spine, chest):
        stitched(g, ("cloth",), 3)


def webbing(model, kind):
    """A belt with pouches, braces, and a rig of pouches over the chest, as packed as a plate carrier."""
    hips, spine, chest = grid(model, "Hips"), grid(model, "Spine"), grid(model, "Chest")
    hips.fill(lambda x, y, z: ((x / 9.2) ** 2 + (z / 6.3) ** 2 <= 1) & (y > 0.2) & (y < 2.6),
              ((-10, 0, -7), (10, 3, 7)), "leather")
    hips.box((-1.5, 0, 5.6), (1.5, 3, 7), "brass")
    hips.box((-0.5, 1, 6.5), (0.5, 2, 7), "leather_dark")
    for x0 in (-8.5, 4.5):
        pouch(hips, (x0, -3.5, 4.0), (x0 + 4, 2.5, 7.0), "leather", "leather_dark")
    # A water bottle at the right hip and a bag at the left.
    hips.rod((9.4, -4, -1), (9.4, 1.5, -1), 2.2, "tan_dark", flat=True)
    hips.rod((9.4, 1.5, -1), (9.4, 2.6, -1), 0.9, "steel", flat=True)
    pouch(hips, (-11.5, -5, -2.5), (-8.2, 1.5, 2.5), "tan", "tan_dark")
    # The chest rig: a panel of six pouches in two rows, a strap over each shoulder.
    chest.box((-8.2, -4, 5.6), (8.2, 6.8, 7.4), "cloth_dark", round_edges=0)
    spine.box((-8.2, 1.5, 5.4), (8.2, 6, 7.2), "cloth_dark")
    for row, y0 in enumerate((-4.5, 1.6)):
        for x0 in (-7.6, -2.4, 2.8):
            kind_of = ("tan", "tan_dark") if (row + int(x0 > 0)) % 2 == 0 else ("leather", "leather_dark")
            pouch(chest, (x0 + 0.3, y0, 7.2), (x0 + 4.5, y0 + 4.4, 9.4), *kind_of)
    for side in (-1, 1):
        chest.path([(side * 5.5, 8.6, 5.5), (side * 6.0, 8.6, -5.6)], 1.0, "cloth_dark", square=True)
        chest.path([(side * 6.0, 8.6, 5.8), (side * 6.6, 3, 7.4)], 1.0, "cloth_dark", square=True)
    chest.box((-8.2, -4, -7.4), (8.2, 6.8, -5.6), "cloth_dark")


# Hands closed round a grip ----------------------------------------------------------------------------------------


def hand(model, name, glove, fingertips=None, radius=GRIP_RADIUS):
    """A gloved hand closed round a grip `radius` thick along its Z (see `grip_at`). The left hand is the right
    turned over."""
    g = grid(model, f"{name}Hand")
    made = type(g)(g.seed, g.patch)
    cx, cy, _ = grip_at(radius)
    made.rounded((0.5, -2.6, 0), (1.9, 3.0, 2.6), glove, power=2.4)
    made.rod((0.3, 0.6, 0), (0.3, -1.0, 0), 2.3, glove)

    def arc(x, y):
        dx, dy = x - cx, y - cy
        return np.hypot(dx, dy), np.degrees(np.arctan2(dy, dx))

    def fingers(x, y, z):
        rho, angle = arc(x, y)
        return (rho >= radius) & (rho <= radius + 1.7) & ((angle < -10) | (angle > 110)) & (np.abs(z) < 2.6)

    made.fill(fingers, ((cx - 4, cy - 4, -3), (cx + 4, cy + 4, 3)), glove)
    for (x, y, z), (material, _) in list(made.cells.items()):
        rho, angle = arc(x + 0.5, y + 0.5)
        if not (radius <= rho <= radius + 1.7 and (angle < -10 or angle > 110)) or x + 0.5 > -0.4:
            continue
        made.put((x, y, z), glove, 1 if (z + 3) % 2 else 3)
        if fingertips and (angle > 110 or angle < -150):
            made.put((x, y, z), fingertips)
    made.path([(-0.5, -1.6, 2.0), (-2.2, -2.6, 2.4), (-3.9, -3.4, 2.0), (-4.4, -4.6, 1.4)], 0.9, glove)
    made.remove(lambda x, y, z: (x - cx) ** 2 + (y - cy) ** 2 < (radius - 0.2) ** 2,
                ((cx - 3, cy - 3, -4), (cx + 3, cy + 3, 4)))
    g.merge(made.mirrored() if name == "Left" else made)


def hand_frame(axis, palm_to_grip, left):
    """How a hand is turned, in the gun's space, to close round a grip along `axis`, `palm_to_grip` the way from
    its palm to the grip."""
    z = np.array(axis, float)
    z /= np.linalg.norm(z)
    n = np.array(palm_to_grip, float)
    n = n - z * (n @ z)
    n /= np.linalg.norm(n)
    x = n if left else -n
    return np.column_stack((x, np.cross(z, x), z))


def wrist_for(frame, grip_middle, left, radius):
    """Where the wrist goes for a hand turned `frame` to hold a grip `radius` thick whose middle is `grip_middle`."""
    at = grip_at(radius) * (np.array((-1, 1, 1)) if left else 1)
    return np.array(grip_middle, float) - frame @ at


# Heads ------------------------------------------------------------------------------------------------------------


# One eye as rows from the top, from the nose outwards: lash, iris (dark, then lighter low down), lights.
EYE = (
    "LLLLL",
    "IWII.",
    "IIII.",
    "JJJW.",
    "JJJJ.",
)
EYE_COLOURS = {"L": "lash", "I": "iris", "J": "iris_light", "W": "eye_light"}


def head_shape(g, material):
    """A big head, a cube with its corners worn round, as chibi figures have."""
    g.rounded((0, 10.8, 0.5), (11.0, 10.4, 9.8), material, power=3.0)


def face(g, mask):
    """Eyes, cheeks, the fringe, and `mask` over the lower face."""
    for side in (-1, 1):
        for row, line in enumerate(EYE):
            y = 13 - row
            for at, mark in enumerate(line):
                if mark == ".":
                    continue
                x = 2 + at if side > 0 else -3 - at
                spot = front_of(g, x, y)
                if spot is not None:
                    g.put((x, y, spot), EYE_COLOURS[mark])
        for x in ((6, 7) if side > 0 else (-7, -8)):
            spot = front_of(g, x, 8)
            if spot is not None:
                g.put((x, 8, spot), "blush")
    # Hair at the brow and over the ears, under the helmet.
    g.paint(lambda x, y, z: (y > 14.5) | ((np.abs(x) > 8.5) & (y > 7) & (z < 5)) | ((z < 3) & (y > 2.5)),
            ((-12, 0, -11), (12, 23, 11)), "hair")
    # The mask: over the nose, mouth and jaw, up to under the eyes, flat across the front as cloth pulled tight is,
    # and laid over the sides of the face.
    for x in range(-11, 11):
        for y in range(0, 8):
            spot = front_of(g, x, y)
            if spot is None:
                continue
            front = max(spot, 9 if abs(x) < 8 else spot)
            for z in range(spot, front + 2):
                g.put((x, y, z), mask)
    g.paint(lambda x, y, z: y < 8, ((-12, -1, -11), (12, 8, 12)), mask, only=("skin",))


def goggles(g, y, z0):
    """Goggles pushed up onto the helmet: two square lenses in steel frames on a strap."""
    for side in (-1, 1):
        x0 = 1 if side > 0 else -6
        g.box((x0 - 0.5, y - 0.5, z0), (x0 + 5.5, y + 4.5, z0 + 2.4), "steel")
        g.box((x0 + 0.5, y + 0.5, z0 + 1), (x0 + 4.5, y + 3.5, z0 + 2.8), "glass")
        g.put((x0 + 1, y + 2, int(z0 + 2)), "glint")
        g.put((x0 + 1, y + 3, int(z0 + 2)), "glint")
    g.box((-1, y + 1, z0 + 0.6), (1, y + 3, z0 + 2.0), "leather_dark")


def rifleman_head(model):
    g = grid(model, "Head")
    head_shape(g, "skin")
    face(g, "mask")
    # A respirator: a round filter on the front of the mask, a steel ring round it, straps to the sides.
    g.rod((0, 3.6, 9), (0, 3.6, 13.2), 2.8, "steel", flat=True)
    g.rod((0, 3.6, 12), (0, 3.6, 13.6), 2.1, "blued", flat=True)
    for x in (-1, 0, 1):
        g.put((x, 3, 13), "steel", 3)
    for side in (-1, 1):
        g.rod((side * 3, 5, 9), (side * 10, 7, 3), 0.6, "leather_dark", square=True)
    # The dish helmet: a low crown and a wide brim, rusting, netting over it and rags tied in.
    g.ball((0, 15.5, 0.6), (12.4, 8.4, 11.8), "helmet", keep=lambda x, y, z: y > 15)
    for x in range(-17, 17):
        for z in range(-16, 18):
            # Squarer than a circle, so its edge, and the shadow it throws on the face, runs straight.
            d = (abs(x + 0.5) / 16.2) ** 4 + (abs(z + 0.5 - 0.6) / 15.4) ** 4
            if d <= 1:
                g.put((x, 15 if d < 0.6 else 14, z), "helmet")
    for (x, y, z), (material, _) in list(g.cells.items()):
        if material == "helmet" and y > 16 and (x + y + z) % 4 == 0 and (x - y + z) % 4 == 0:
            g.put((x, y, z), "scrim", 0)
        elif material == "helmet" and hashed(x // 2, y, z // 2, 77) < 0.08:
            g.put((x, y, z), "rust")
    for i in range(16):
        a = hashed(i, 5) * math.tau
        r = 6 + hashed(i, 6) * 7
        x, z = int(math.cos(a) * r), int(math.sin(a) * r)
        top = max((y for (cx, y, cz) in g.cells if cx == x and cz == z), default=None)
        if top is not None:
            g.put((x, top + 1, z), "scrim")
    goggles(g, 17.5, 9.4)
    g.box((-11.5, 17, -11), (11.5, 18.5, 11), "leather_dark", only=("helmet", "scrim", "rust"))
    # The chin strap.
    g.path([(-10.4, 14, 3), (-9.6, 4, 5), (-6, 0.6, 6)], 0.6, "leather")
    g.path([(10.4, 14, 3), (9.6, 4, 5), (6, 0.6, 6)], 0.6, "leather")


def scavenger_head(model):
    g = grid(model, "Head")
    head_shape(g, "skin")
    face(g, "mask")
    # A steel helmet, deep, its skirt flaring over the ears and the neck; a headset's ear cups over it.
    def bowl(x, y, z):
        return (y > 13.5) & ((x / 12.8) ** 2 + ((y - 13.5) / 9.6) ** 2 + ((z - 0.3) / 12.2) ** 2 <= 1)
    g.fill(bowl, ((-14, 13, -13), (14, 24, 13)), "helmet")
    for x in range(-15, 15):
        for z in range(-15, 13):
            r = math.hypot((x + 0.5) / 12.6, (z + 0.5 - 0.3) / 12.0)
            if 0.86 < r <= 1.16 and (z < 6 or abs(x) > 8):
                drop = 0 if z > 5 else 4 if z < -4 else 2
                for y in range(14 - drop, 15):
                    g.put((x, y, z), "helmet")
    g.box((-9, 13.5, 9), (9, 14.5, 13), "helmet")  # the peak
    for (x, y, z), (material, _) in list(g.cells.items()):
        if material == "helmet" and hashed(x // 2, y // 2, z // 2, 41) < 0.08:
            g.put((x, y, z), "rust")
    for side in (-1, 1):
        g.rounded((side * 12.8, 8.5, 1), (2.4, 3.6, 3.6), "steel", power=2.4)
        g.rounded((side * 14.0, 8.5, 1), (1.4, 2.6, 2.6), "tan_dark", power=2.4)
        g.path([(side * 12, 12, 1), (side * 12.8, 16, 1)], 0.7, "leather_dark", square=True)
    # A microphone on a stalk round to the mouth.
    g.path([(11.5, 7, 4), (10, 4.5, 8), (5, 3.5, 11)], 0.6, "blued")
    g.ball((4.2, 3.5, 11.2), (1.2, 1.2, 1.2), "blued")
    goggles(g, 17.0, 10.0)
    g.box((-13, 16.6, -12.5), (13, 18.4, 12.5), "leather_dark", only=("helmet", "rust"))


# Gear on the back ------------------------------------------------------------------------------------------------


def rifleman_gear(model):
    chest = grid(model, "Chest")
    chest.box((-7.5, -3.5, -12), (7.5, 7, -6.5), "tan", round_edges=1)
    chest.box((-7.5, 4, -12.5), (7.5, 7, -6.5), "tan_dark")
    for x in (-4, 3):
        chest.box((x, -2, -12.8), (x + 1.2, 6, -12), "leather")
        chest.put((x, 1, -13), "brass", 1)
    chest.rod((-9.5, 9, -9), (9.5, 9, -9), 2.6, "rag")
    for x in (-6, 5):
        chest.rod((x, 9, -9), (x + 1, 9, -9), 2.9, "leather_dark", flat=True)
    chest.box((-3, -2.5, -13.5), (3, 1.5, -12), "steel")


def scavenger_gear(model):
    chest = grid(model, "Chest")
    # A field radio on the back, its dials and two aerials standing up past the head.
    chest.box((-7.5, -4, -13), (7.5, 8, -6.5), "tan_dark", round_edges=1)
    chest.box((-6, -2, -13.6), (6, 6, -12.8), "steel")
    for x in (-4, -1, 2):
        chest.put((x, 3, -14), "brass", 3)
    chest.box((-5, -1, -14), (5, 1, -13.6), "blued")
    for x, height in ((-5.5, 26), (5.5, 20)):
        chest.rod((x, 8, -10), (x, 8 + height, -10.5), 0.55, "blued", square=True)
        chest.rod((x, 8, -10), (x, 10, -10), 1.1, "blued")
    # A bandolier round from the right shoulder, rounds in it.
    chest.path([(7.5, 8.6, 6.2), (-1, 2, 9.6), (-8.6, -3.4, 7.4)], 1.0, "leather", square=True)
    for t in np.linspace(0.1, 0.9, 7):
        x, y = 7.5 - 16 * t, 8.6 - 12 * t
        spot = front_of(chest, int(x), int(y))
        if spot is not None:
            chest.put((int(x), int(y), spot + 1), "brass", 2)


# Guns -------------------------------------------------------------------------------------------------------------


def rifle(g):
    """A bolt-action rifle along +Z, chunky as the style wants, the middle of its wrist at the origin. Gives where
    each hand holds it: for each, the grip's axis, the way from the palm to it, and its middle."""
    wrist_axis = np.array((0.0, 0.77, 0.64))
    g.rod(-wrist_axis * 2.8, wrist_axis * 2.8, GRIP_RADIUS, "wood")
    # A short stock, as the style wants, that ends at the shoulder when the hand is on the grip.
    for z in range(-12, -1):
        t = (z + 12) / 11
        g.box((-1.5, -5.2 + t * 3.8, z), (1.5, 2.2 - t * 0.2, z + 1), "wood")
    g.box((-1.6, -5.6, -13), (1.6, 2.4, -12), "brass", shade=1)
    g.box((-1.6, 1.0, -1), (1.6, 4.2, 9), "blued")
    g.box((-1.3, -2.8, 3), (1.3, 1.0, 7), "blued")
    g.path([(0, 0.5, 0.6), (0, -2.0, 1.4), (0, -2.0, 3)], 0.5, "blued")
    g.rod((0, 4.0, -1), (0, 4.0, 4), 1.0, "steel")
    g.path([(0.8, 3.8, 0.5), (2.8, 2.8, 0.8)], 0.5, "steel")
    g.ball((3.2, 2.6, 0.9), (0.9, 0.9, 0.9), "steel", shade=3)
    g.rod((0, 1.0, 8), (0, 1.0, 25), 1.6, "wood", flat=True)
    g.box((-1.2, 3.0, 9), (1.2, 4.2, 22), "wood")
    g.rod((0, 3.4, 9), (0, 3.4, 31), 0.8, "blued")
    g.box((-1.3, -0.6, 25), (1.3, 4.6, 28), "blued")
    g.box((-0.5, 4.6, 26), (0.5, 5.8, 27), "blued")
    g.box((-1.2, 4.2, 9), (1.2, 5.6, 11), "blued")
    for z in (17, 22):
        g.box((-1.9, -1.2, z), (1.9, 4.4, z + 1), "brass", shade=0)
    g.rod((0, 1.0, 9.5), (0, 1.0, 15.5), 1.7, "rag", flat=True)
    for (x, y, z), (material, _) in list(g.cells.items()):
        if material == "rag" and z % 2:
            g.put((x, y, z), "rag", 0)
    g.path([(0, -5.0, -10), (0, -6.5, -4), (0, -5, 6), (0, -1.5, 17)], 0.5, "leather", square=True)
    return {
        "right": (wrist_axis, (-0.85, -0.5, 0.0), np.zeros(3), GRIP_RADIUS),
        "carry": ((0.0, 0.0, 1.0), (0.0, 1.0, 0.0), np.array((0.0, 1.0, 9.5)), 1.7),
        "aim": ((0.0, 0.0, 1.0), (0.0, 1.0, 0.0), np.array((0.0, 1.0, 10.0)), 1.7),
        "stock": np.array((0.0, -1.6, -12.5)),
    }


def smg(g):
    """A drum-fed submachine gun along +Z, the middle of its pistol grip at the origin, a vertical grip under its
    barrel for the other hand. Gives where each hand holds it, as `rifle` does."""
    grip_axis = np.array((0.0, 0.8, 0.6))
    g.rod(-grip_axis * 2.8, grip_axis * 2.6, GRIP_RADIUS, "wood")
    g.box((-1.5, -3.8, -1.8), (1.5, -2.6, 0.4), "wood", round_edges=1)
    g.box((-1.8, 2.4, -2), (1.8, 6, 12), "blued")
    g.box((-1.3, 6, 1), (1.3, 6.8, 8), "steel")
    g.box((-0.5, 6.4, -0.5), (0.5, 7.6, 0.8), "blued")
    g.box((1.8, 4, 5), (2.8, 5, 6.2), "steel", shade=3)
    for z in range(-11, -2):
        t = (z + 11) / 9
        g.box((-1.4, 2.2 - (1 - t) * 3.4, z), (1.4, 5.6, z + 1), "wood")
    g.box((-1.5, -1.4, -12), (1.5, 5.8, -11), "steel", shade=0)
    g.path([(0, 2.4, 0.8), (0, 0.6, 1.6), (0, 0.6, 3.8), (0, 2.4, 4.2)], 0.5, "blued")
    g.rod((0, 4.2, 12), (0, 4.2, 21), 1.3, "blued", flat=True)
    for z in range(13, 20, 2):
        g.rod((0, 4.2, z), (0, 4.2, z + 0.9), 1.8, "steel", shade=0, flat=True)
    g.box((-1.2, 3.0, 21), (1.2, 5.4, 23), "blued")
    g.box((-0.5, 5.4, 21.5), (0.5, 6.4, 22.5), "blued")
    g.rod((-1.9, -1.0, 6.5), (1.9, -1.0, 6.5), 3.6, "blued", flat=True)
    g.rod((-2.1, -1.0, 6.5), (2.1, -1.0, 6.5), 1.1, "steel", flat=True)
    for (x, y, z), (material, _) in list(g.cells.items()):
        if material == "blued" and abs(x + 0.5) >= 1.8 and hashed(x, y, z, 31) < 0.15:
            g.put((x, y, z), "rust")
    fore_axis = np.array((0.0, 0.98, 0.18))
    top = np.array((0, 3.0, 12.5))
    g.rod(top - fore_axis * 5.8, top, GRIP_RADIUS, "wood")
    middle = top - fore_axis * 3.0
    return {
        "right": (grip_axis, (-0.85, -0.5, 0.0), np.zeros(3), GRIP_RADIUS),
        "carry": (fore_axis, (1.0, 0.0, 0.0), middle, GRIP_RADIUS),
        "aim": (fore_axis, (1.0, 0.0, 0.0), middle, GRIP_RADIUS),
        "stock": np.array((0.0, 2.0, -11.5)),
    }


# Posing: the gun placed, the hands put on it -----------------------------------------------------------------------


def whole(model, body_keys):
    """Keys in degrees from rest as whole turnings."""
    return {name: (turned(model.by_name(name).rotation) @ turned(r), m) for name, (r, m) in body_keys.items()}


def hold(model, grips, gun_r, gun_t, body_keys, hold_with, tag):
    """Every bone's turning, whole, for the gun turned `gun_r` with its origin at `gun_t` in the model's space, the
    body posed by `body_keys` (degrees from rest), the right hand on the grip and the left where `hold_with` says."""
    keys = whole(model, body_keys)
    placed = model.placed(keys)
    gun = model.by_name("Gun")
    pr, pt = placed[gun.parent]
    keys["Gun"] = (pr.T @ gun_r, list(pr.T @ (np.array(gun_t, float) - pt) - gun.pivot))
    placed = model.placed(keys)
    shortfalls = []
    right_pole, left_pole = POLES[hold_with]
    for name, left, (axis, palm, middle, radius), pole in (
        ("Right", False, grips["right"], right_pole),
        ("Left", True, grips[hold_with], left_pole),
    ):
        frame = hand_frame(axis, palm, left)
        wrist_r = gun_r @ frame
        wrist_t = np.array(gun_t, float) + gun_r @ wrist_for(frame, middle, left, radius)
        turns, short = two_bone(model, f"{name}UpperArm", f"{name}Forearm", f"{name}Hand", wrist_r, wrist_t, pole,
                                placed)
        for bone, rotation in turns.items():
            keys[bone] = (rotation, (0, 0, 0))
        placed = model.placed(keys)
        shortfalls.append(short)
    if max(shortfalls) > 0.5:
        print(f"  {tag}: the hands fall short by {shortfalls[0]:.1f} and {shortfalls[1]:.1f} cubes")
    return keys


# How the gun is held at rest, for each: the way its muzzle points, its top, and where the right hand's grip is.
CARRY = {
    "rifleman": ((-0.65, -0.55, 0.52), (0.15, 1.0, 0.35), (7.0, 22.0, 9.0)),
    "scavenger": ((-0.65, -0.55, 0.52), (0.15, 1.0, 0.35), (7.0, 22.0, 9.0)),
}
# Which way each elbow points, the right's then the left's, at rest and aiming.
POLES = {"carry": ((0.6, -1.0, -0.8), (-0.4, -1.0, -0.6)), "aim": ((1.0, 0.0, -0.4), (-0.3, -1.0, -0.2))}
# Aiming: how far the hips turn side on, and where in the chest the stock sits.
AIM = {"rifleman": (50.0, (8.5, 3.5, 8.0)), "scavenger": (50.0, (8.5, 3.5, 8.0))}


def carry(kind):
    """The gun held low across the body at rest, its stock by the right shoulder and its muzzle down to the left."""
    forward, top, grip = CARRY[kind]
    return basis(forward, top), np.array(grip, float)


def aim_body(kind, sway=0.0, kick=0.0):
    """The body turned side on to the aim, the head kept to the front."""
    turn = AIM[kind][0]
    return {
        "Hips": ((0.0, turn, 0.0), (0, -0.6, 0)),
        "Spine": ((2.0 - kick * 2, -2.0, 0.0), (0, 0, 0)),
        "Chest": ((3.0 - kick * 3 + sway * 0.6, -4.0, 0.0), (0, 0, 0)),
        "Neck": ((2.0, -8.0, 0.0), (0, 0, 0)),
        "Head": ((4.0 - kick * 3, -(turn - 14.0), 3.0), (0, 0, 0)),
        "LeftThigh": ((-14.0, -(turn - 6.0), -3.0), (0, 0, 0)),
        "LeftShin": ((10.0, 0, 0), (0, 0, 0)),
        "LeftFoot": ((4.0, 10.0, 0), (0, 0, 0)),
        "RightThigh": ((8.0, -(turn + 2.0), 6.0), (0, 0, 0)),
        "RightShin": ((8.0, 0, 0), (0, 0, 0)),
        "RightFoot": ((-4.0, 20.0, 0), (0, 0, 0)),
    }


def aim_gun(kind, model, grips, sway=0.0, kick=0.0):
    """The gun level at the right shoulder, its stock in the shoulder's pocket, pointing ahead."""
    placed = model.placed(whole(model, aim_body(kind, sway, kick)))
    cr, ct = placed["Chest"]
    pocket = ct + cr @ np.array(AIM[kind][1])
    up = math.radians(-1.0 + sway * 0.8 + kick * 9.0)
    gun_r = rot_y(math.radians(sway * 0.6)) @ rot_x(-up)
    return gun_r, pocket - gun_r @ grips["stock"] + np.array((0.0, 0.0, -kick * 1.2))


def make(kind):
    model = Model(palette(kind), pixels_per_block=PIXELS)
    skeleton(model)
    for bone in model.bones:
        bone.grid.patch = 2
    body(model, kind)
    webbing(model, kind)
    if kind == "rifleman":
        rifleman_head(model)
        rifleman_gear(model)
    else:
        scavenger_head(model)
        scavenger_gear(model)
    gun = model.bone("Gun", "Chest")
    gun.grid.patch = 2
    grips = rifle(gun.grid) if kind == "rifleman" else smg(gun.grid)
    for name, held in (("Left", "carry"), ("Right", "right")):
        hand(model, name, "glove", fingertips="skin" if kind == "scavenger" else None, radius=grips[held][3])

    # At rest the arms hold the gun low across the body: that pose's turnings are the bones' rest.
    gun_r, gun_t = carry(kind)
    for name, (rotation, move) in hold(model, grips, gun_r, gun_t, {}, "carry", f"{kind} carrying").items():
        bone = model.by_name(name)
        bone.rotation = degrees_of(rotation)
        bone.pivot = bone.pivot + np.array(move, float)

    def posed(sway=0.0, kick=0.0):
        gun_r, gun_t = aim_gun(kind, model, grips, sway, kick)
        keys = hold(model, grips, gun_r, gun_t, aim_body(kind, sway, kick), "aim", f"{kind} aiming")
        return {name: (degrees_of(turned(model.by_name(name).rotation).T @ rotation), move)
                for name, (rotation, move) in keys.items()}

    aim = [(t, posed(sway=s)) for t, s in ((0.0, 0.0), (0.6, 1.0), (1.2, 0.0), (1.8, -1.0))]
    fire = [(t, posed(kick=k)) for t, k in ((0.0, 0.0), (0.05, 1.0), (0.18, 0.4), (0.4, 0.0))]
    model.clips.append(clip("Aim", 2.4, True, aim))
    model.clips.append(clip("Fire", 0.4, False, fire))
    return model


def clip(name, length, looping, timed):
    bones = {}
    for bone in sorted({bone for _, keys in timed for bone in keys}):
        keyed = []
        for t, keys in timed:
            rotation, move = keys.get(bone, ((0, 0, 0), (0, 0, 0)))
            keyed.append((t, [round(float(v), 2) for v in rotation], [round(float(v), 2) for v in move]))
        bones[bone] = keyed
    return {"name": name, "length": length, "looping": looping, "bones": bones}


# What the gun and the arms must keep out of, to tell how far a pose has them clip into the body.
BODY = ["Hips", "Spine", "Chest", "Neck", "Head", "LeftThigh", "RightThigh", "LeftShin", "RightShin"]
HELD = ["Gun", "LeftForearm", "RightForearm", "LeftHand", "RightHand"]


def clipped(model, keys):
    """How many cubes of the gun, forearms and hands lie inside the body in a pose, and of the gun inside the
    arms. A few are where the stock sits in the shoulder; many mean the pose wants changing."""
    arms = ["LeftUpperArm", "RightUpperArm", "LeftForearm", "RightForearm"]
    return clipping(model, keys, HELD, BODY) + clipping(model, keys, ["Gun"], arms)


def main():
    for kind, file in (("rifleman", "Rifleman"), ("scavenger", "Scavenger")):
        model = make(kind)
        write_model(model, os.path.join(HERE, f"{file}.boxmodel"))
        cubes = sum(len(bone.grid.cells) for bone in model.bones)
        aiming = {bone: (tuple(keys[0][1]), tuple(keys[0][2])) for bone, keys in model.clips[0]["bones"].items()}
        print(f"{file}: {cubes} cubes, {len(model.palette.colors)} colours, {len(model.bones)} bones; "
              f"cubes clipping into the body {clipped(model, {})} at rest, {clipped(model, aiming)} aiming")


if __name__ == "__main__":
    main()
