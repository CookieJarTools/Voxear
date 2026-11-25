#!/usr/bin/env python3
"""Makes the Wasteland soldiers as box models of small cubes, with a humanoid's bones and their aim clips.

    python3 make_soldiers.py    # writes Rifleman.boxmodel and Scavenger.boxmodel beside this file

Each soldier is a chibi figure 128 cubes and 1.9 m tall: a helmeted head nearly a third of that and as wide as the
shoulders, large eyes over a mask, a short body in a rig packed with pouches and straps, chunky arms and legs, knee
pads and big boots, each part built of plates and blocks of their own tones. The bones are named as a humanoid rig
names them (Hips, Spine, Chest, Neck, Head, LeftUpperArm, LeftForearm, LeftHand, LeftThigh, LeftShin, LeftFoot and
the right side's), so the engine finds its arms, legs and spine.

The hands have four fingers and a thumb closed round the parts of the gun they hold, and every pose puts each hand
exactly there: the arm reaches its wrist to the place and turning the grip asks for, the elbow bending out and the
forearm twisting to spare the wrist. At rest the gun is held across the body, so Mob Motion walks, runs and idles
the body and legs while the hands stay on it. Aiming is a clip in each model ("Aim"), with "Fire" for the kick of
a shot. Running this again makes the models afresh, losing changes made in the Model window.
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
PIXELS = 67.0  # cubes to a metre: 128 tall is 1.9 m

# Where the bones stand, in cubes: the hips from the ground, each other from the one above.
HIPS = 50
SPINE = 7
CHEST = 10
NECK = 20
HEAD = 2
SHOULDER = (9.0, 13.0)
UPPER_ARM = 12.0
ELBOW = 18.0
WRIST = 17.0
THIGH = (8.0, -3.0)
KNEE = 19.0
ANKLE = 17.0

# A hand closed round a grip, in the hand's own space: the wrist at its pivot, the fingers reaching down -Y, the
# palm facing -X (a right hand; the left is it turned over), the thumb and index finger to +Z. The grip it holds
# runs along Z through `grip_at`.
GRIP_RADIUS = 2.2


def grip_at(radius):
    """Where a hand's grip runs, for a grip `radius` thick: just off the palm, under the knuckles."""
    return np.array((-0.8 - radius, -9.0, 0.0))


def palette(kind):
    p = Palette()
    shades = (0.7, 0.85, 1.0, 1.15)
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
    p.add("webbing", "2e2c2a", shades=shades)
    if kind == "rifleman":
        p.add("cloth", "6a6846", shades=shades, warm=0.03)
        p.add("cloth_dark", "4a4934", shades=shades, warm=0.03)
        p.add("puttee", "8a7c58", shades=shades)
        p.add("helmet", "5e6046", shades=shades, warm=0.02)
        p.add("mask", "3a3e38", shades=shades)
        p.add("patch", "7a3e32", shades=shades)
        p.add("scrim", "7d7a52", shades=shades)
        p.add("glove", "3a2c22", shades=shades, warm=0.03)
    else:
        p.add("cloth", "4c5572", shades=shades, warm=-0.03)
        p.add("cloth_dark", "353b52", shades=shades, warm=-0.03)
        p.add("puttee", "5a6078", shades=shades)
        p.add("helmet", "4e5668", shades=shades, warm=-0.02)
        p.add("mask", "2e3348", shades=shades)
        p.add("patch", "35405e", shades=shades)
        p.add("scrim", "6a6a5a", shades=shades)
        p.add("glove", "2a2624", shades=shades, warm=0.03)
    return p


def grid(model, name):
    return model.by_name(name).grid


def front_of(g, x, y):
    zs = [z for (cx, cy, z) in g.cells if cx == x and cy == y]
    return max(zs) if zs else None


def pouch(g, lo, hi, material="tan", flap="tan_dark", stud=True):
    """A pouch: a block, a flap over its top standing out a little, a stud on the flap, a seam down its side."""
    g.box(lo, hi, material, round_edges=0.6)
    g.box((lo[0] - 0.3, hi[1] - 2.2, lo[2]), (hi[0] + 0.3, hi[1] + 0.4, hi[2] + 0.8), flap, round_edges=0.5)
    if stud:
        g.box(((lo[0] + hi[0]) / 2 - 0.6, hi[1] - 1.6, hi[2] + 0.6), ((lo[0] + hi[0]) / 2 + 0.6, hi[1] - 0.5,
              hi[2] + 1.4), "brass", shade=2)
    g.box((lo[0] + 0.6, lo[1] + 0.6, hi[2] - 0.2), (lo[0] + 1.4, hi[1] - 2.4, hi[2] + 0.2), flap, shade=0)


def band(g, y0, y1, half_x, half_z, material, out=0.6):
    """A strap round a limb or the body, between heights `y0` and `y1`, standing out a little."""
    hx, hz = half_x + out, half_z + out
    g.fill(lambda x, y, z: ((x / hx) ** 2 + (z / hz) ** 2 <= 1) & ((x / (hx - 1.6)) ** 2 + (z / (hz - 1.6)) ** 2 > 1),
           ((-hx - 1, y0, -hz - 1), (hx + 1, y1, hz + 1)), material)


def chevron(g, x0, y0, z, width, material, side=1, axis="z"):
    """A V of two stripes, as a badge has: on a face looking along +Z, or along +X or -X with `axis` "x"."""
    for i in range(int(width // 2) + 1):
        for dy in (0, 1):
            for dx in (i, width - 1 - i):
                if axis == "z":
                    g.put((x0 + dx, y0 + i + dy, z), material, 3)
                else:
                    g.put((z, y0 + i + dy, x0 + dx), material, 3)


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
    for bone in model.bones:
        bone.grid.panels = True


# The body both soldiers share -------------------------------------------------------------------------------------


def body(model, kind):
    hips, spine, chest = grid(model, "Hips"), grid(model, "Spine"), grid(model, "Chest")
    hips.rounded((0, -1, 0), (15.0, 7.0, 10.5), "cloth", power=2.8)
    spine.rounded((0, 4, 0), (14.5, 7.5, 10.5), "cloth", power=2.8)
    chest.rounded((0, 8, 0), (18.0, 12.0, 11.5), "cloth", power=2.8)
    chest.rounded((0, 15, 0), (21.0, 6.0, 10.0), "cloth", power=2.4)
    chest.rod((0, 19, -0.5), (0, 23.5, -0.5), 7.6, "cloth_dark", flat=True)
    chest.remove(lambda x, y, z: (x * x + (z + 0.5) ** 2 < 5.4 ** 2) & (y > 19), ((-9, 19, -9), (9, 24, 9)))
    grid(model, "Neck").rod((0, -3, 0), (0, 4, 0.5), 6.0, "skin")
    # Arms: a padded shoulder with a badge, the sleeve, a cuff, the glove's wrist.
    for side, name in ((-1, "Left"), (1, "Right")):
        upper = grid(model, f"{name}UpperArm")
        upper.ball((0, -2, 0), (8.4, 7.6, 8.4), "cloth")
        upper.rounded((0, -ELBOW / 2 - 0.5, 0), (7.2, ELBOW / 2, 7.2), "cloth", power=2.5)
        badge = (side * 6.2, -10, -4), (side * 8.6, -2, 4)
        upper.box(tuple(min(a, b) for a, b in zip(*badge)), tuple(max(a, b) for a, b in zip(*badge)), "patch",
                  round_edges=0.5)
        chevron(upper, -2, -8, int(side * 8.6) - (0 if side > 0 else 1), 5, "tan", axis="x")
        band(upper, -ELBOW + 1, -ELBOW + 3, 6.9, 6.9, "cloth_dark", out=0.4)
        fore = grid(model, f"{name}Forearm")
        fore.ball((0, 0, 0), (6.8, 6.8, 6.8), "cloth")
        fore.rounded((0, -(WRIST - 3) / 2, 0), (6.6, (WRIST - 1) / 2, 6.6), "cloth", power=2.5)
        fore.rod((0, -WRIST + 6.5, 0), (0, -WRIST + 3.8, 0), 7.0, "cloth_dark", flat=True)
        fore.rod((0, -WRIST + 4.0, 0), (0, -WRIST + 1.0, 0), 4.6, "glove", flat=True)
    # Legs: the thigh with straps, a knee pad, puttees wound to the boot, and the boot itself.
    for side, name in ((-1, "Left"), (1, "Right")):
        thigh = grid(model, f"{name}Thigh")
        thigh.rounded((0, -8, 0), (8.8, 11.5, 9.0), "cloth", power=2.6)
        band(thigh, -6, -4, 8.8, 9.0, "webbing")
        if side > 0:
            pouch(thigh, (8.2, -15, -4.5), (12.4, -6, 4.5))
        else:
            band(thigh, -14, -12, 8.4, 8.6, "webbing")
        shin = grid(model, f"{name}Shin")
        shin.rounded((0, -1, 0), (8.0, 5.0, 8.0), "cloth", power=2.6)
        shin.rounded((0, -10, 0), (7.4, 8.0, 7.4), "puttee", power=2.4)
        for (x, y, z), (material, _) in list(shin.cells.items()):
            if material == "puttee" and (y + round(math.atan2(z, x) / math.tau * 3)) % 3 == 0:
                shin.put((x, y, z), "puttee", 0)
        shin.rounded((0, -2.5, 7.2), (6.4, 5.8, 2.6), "leather", power=2.6)
        shin.box((-5.6, -4.5, 8.9), (5.6, -0.5, 10.2), "leather", shade=3)
        band(shin, -3, -1, 7.8, 7.8, "webbing")
        foot = grid(model, f"{name}Foot")
        foot.rod((0, -2, -0.5), (0, 7, -0.5), 7.2, "boot", flat=True)
        foot.rounded((0, -5.5, 3.5), (8.0, 5.5, 13.0), "boot", power=2.6)
        foot.box((-8.4, -11, -9.8), (8.4, -8.6, 16.9), "sole", round_edges=1)
        foot.box((-8.2, -8.6, 9), (8.2, -3.5, 16.6), "boot", only=("boot",), shade=3)
        foot.box((-8.6, 3, -8), (8.6, 5, 7), "leather_dark", only=None)
        for y in range(-5, 6, 2):
            for x in (-2, -1, 0, 1):
                spot = front_of(foot, x, y)
                if spot is not None and spot > 5:
                    foot.put((x, y, spot), "tan_dark" if x in (-2, 1) else "boot", 1)


def rig(model, kind):
    """The belt and its pouches, and a rig over the chest packed as a plate carrier is: a dark panel of straps and
    two rows of pouches, a back panel with a badge, straps over the shoulders."""
    hips, spine, chest = grid(model, "Hips"), grid(model, "Spine"), grid(model, "Chest")
    hips.fill(lambda x, y, z: ((x / 16.4) ** 2 + (z / 12.0) ** 2 <= 1) & (np.abs(y - 4) < 2.3),
              ((-17, 1, -13), (17, 7, 13)), "leather")
    hips.box((-2.6, 1.4, 11), (2.6, 6.6, 13.2), "brass")
    hips.box((-1.2, 2.6, 12.6), (1.2, 5.4, 13.4), "leather_dark")
    for x0 in (-14.5, 8.5):
        pouch(hips, (x0, -4, 8.6), (x0 + 6, 5, 13.6), "leather", "leather_dark")
    pouch(hips, (-6, -5, -14.5), (6, 4, -10.5), "tan", "tan_dark")
    hips.rod((16.5, -7, -2), (16.5, 2, -2), 3.6, "tan_dark", flat=True)
    hips.rod((16.5, 2, -2), (16.5, 4, -2), 1.6, "steel", flat=True)
    # The front panel, its webbing in rows, and two rows of three pouches.
    chest.box((-15, -7, 9.4), (15, 17, 13.2), "webbing", round_edges=1)
    spine.box((-14.5, 1, 8.8), (14.5, 11, 12.6), "webbing", round_edges=1)
    for y in range(-5, 17, 3):
        chest.box((-14.6, y, 13.0), (14.6, y + 1, 13.6), "webbing", shade=3)
    for row, y0 in enumerate((4.0, -6.0)):
        for col, x0 in enumerate((-14.0, -4.5, 5.0)):
            tone = ("tan", "tan_dark") if (row + col) % 2 == 0 else ("leather", "leather_dark")
            pouch(chest, (x0 + 0.3, y0, 13.2), (x0 + 9.0, y0 + 8.4, 17.4), *tone)
    pouch(chest, (-3.5, 13, 13.0), (3.5, 17.5, 15.6), "tan_dark", "webbing", stud=False)
    # Straps over the shoulders, and the back panel with a badge.
    for side in (-1, 1):
        chest.box((side * 6 - 2.6, 17.5, -11), (side * 6 + 2.6, 20.8, 12.5), "webbing", round_edges=0.6)
    chest.box((-14.5, -6, -14.5), (14.5, 17, -10.4), "webbing", round_edges=1)
    chest.box((-4.5, 8, -15.4), (4.5, 14, -14.2), "patch", round_edges=0.4)
    chevron(chest, -3, 9, -16, 7, "eye_light", axis="z")


# Hands closed round a grip ----------------------------------------------------------------------------------------


def hand(model, name, glove, fingertips=None, radius=GRIP_RADIUS):
    """A gloved hand, four fingers and a thumb closed round a grip `radius` thick along its Z (see `grip_at`). The
    left hand is the right turned over."""
    g = grid(model, f"{name}Hand")
    made = type(g)(g.seed, g.patch)
    made.panels = True
    cx, cy, _ = grip_at(radius)
    # The back of the hand and the palm, and the glove's cuff at the wrist.
    made.rounded((1.4, -4.6, 0), (2.8, 5.4, 4.4), glove, power=2.4)
    made.rod((1.0, 1.4, 0), (1.0, -1.6, 0), 4.4, glove, flat=True)
    made.rod((1.0, 1.4, 0), (1.0, 0.2, 0), 4.7, "leather_dark", flat=True)
    thick = 2.4
    # Four fingers, each two cubes wide, wrapped from the palm under the grip and up its far side, the middle two
    # reaching furthest; a knuckle ridge where they leave the palm.
    for k in range(4):
        z0 = -4 + 2 * k
        reach = 118 if k in (1, 2) else 128
        tone = 1 if k % 2 else 3

        def finger(x, y, z, z0=z0, reach=reach):
            dx, dy = x - cx, y - cy
            rho = np.hypot(dx, dy)
            angle = np.degrees(np.arctan2(dy, dx))
            wrapped = (angle < -12) | (angle > reach)
            return (rho >= radius) & (rho <= radius + thick) & wrapped & (z >= z0) & (z < z0 + 2) & (x < 0.5)

        made.fill(finger, ((cx - radius - thick - 1, cy - radius - thick - 1, z0), (1, cy + radius + thick + 1,
                                                                                   z0 + 2)), glove, shade=tone)
        if fingertips:
            def tip(x, y, z, z0=z0, reach=reach):
                dx, dy = x - cx, y - cy
                angle = np.degrees(np.arctan2(dy, dx))
                rho = np.hypot(dx, dy)
                return ((angle > reach) & (angle < reach + 30)) & (rho >= radius) & (rho <= radius + thick) & \
                    (z >= z0) & (z < z0 + 2)
            made.paint(tip, ((cx - 6, cy - 6, z0), (cx + 6, cy + 6, z0 + 2)), fingertips)
    made.box((-0.6, cy - 1.2, -4), (0.6, cy + 1.6, 4), glove, shade=0)
    made.path([(0.2, -3.2, 4.0), (-2.4, -5.0, 4.9), (-5.0, -6.8, 4.4), (-6.6, -9.0, 3.0)], 1.5, glove)
    if fingertips:
        made.ball((-6.6, -9.0, 3.0), (1.6, 1.6, 1.6), fingertips)
    made.remove(lambda x, y, z: (x - cx) ** 2 + (y - cy) ** 2 < (radius - 0.2) ** 2,
                ((cx - 5, cy - 5, -6), (cx + 5, cy + 5, 6)))
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


# One eye as rows from the top, from the nose outwards: lashes, the iris dark above and lighter below, two lights.
EYE = (
    "LLLLLLLL",
    "LLLLLLL.",
    "IWWIIII.",
    "IWWIIII.",
    "IIIIIII.",
    "IIIIIWI.",
    "JJJJJJJ.",
    "JJJJJJ..",
)
EYE_COLOURS = {"L": "lash", "I": "iris", "J": "iris_light", "W": "eye_light"}


def head_shape(g):
    g.rounded((0, 16, 1), (17.5, 16.0, 15.0), "skin", power=3.0)


def face(g, mask):
    """Eyes, cheeks, hair under the helmet, and `mask` over the lower face, flat across its front."""
    for side in (-1, 1):
        for row, line in enumerate(EYE):
            y = 22 - row
            for at, mark in enumerate(line):
                if mark == ".":
                    continue
                x = 3 + at if side > 0 else -4 - at
                spot = front_of(g, x, y)
                if spot is not None:
                    g.put((x, y, spot), EYE_COLOURS[mark])
        for x in ((11, 12, 13) if side > 0 else (-12, -13, -14)):
            spot = front_of(g, x, 14)
            if spot is not None:
                g.put((x, 14, spot), "blush")
    g.paint(lambda x, y, z: (y > 24.5) | ((np.abs(x) > 13.5) & (y > 12) & (z < 9)) | ((z < 4) & (y > 5)),
            ((-17, 0, -15), (17, 35, 17)), "hair")
    for x in range(-16, 16):
        for y in range(0, 14):
            spot = front_of(g, x, y)
            if spot is None:
                continue
            front = max(spot, 17 if abs(x) < 12 else spot)
            for z in range(spot, front + 2):
                g.put((x, y, z), mask, 1 if (y // 3 + x // 6) % 2 else 2)
    g.paint(lambda x, y, z: y < 14, ((-17, -1, -15), (17, 14, 19)), mask, only=("skin",))


def goggles(g, y, z0, width=10):
    """Goggles up on the helmet: two lenses in steel frames on a strap, a bridge between them."""
    for side in (-1, 1):
        x0 = 1 if side > 0 else -1 - width
        g.box((x0 - 0.6, y - 0.6, z0), (x0 + width + 0.6, y + 6.6, z0 + 3.2), "steel", round_edges=0.6)
        g.box((x0 + 0.8, y + 0.8, z0 + 1.6), (x0 + width - 0.8, y + 5.2, z0 + 3.6), "glass")
        g.box((x0 + 1.2, y + 3.4, z0 + 3.0), (x0 + 2.6, y + 4.8, z0 + 3.8), "glint")
    g.box((-1.6, y + 1.5, z0 + 0.6), (1.6, y + 4.5, z0 + 2.6), "leather_dark")


def rifleman_head(model):
    g = grid(model, "Head")
    head_shape(g)
    face(g, "mask")
    # A respirator: a round filter out from the mask, a steel ring and ribs round it, straps back to the sides.
    g.rod((0, 7, 18), (0, 7, 23.6), 5.0, "steel", flat=True)
    g.rod((0, 7, 22.6), (0, 7, 24.4), 3.6, "blued", flat=True)
    for z in (19.5, 21.5):
        g.rod((0, 7, z), (0, 7, z + 0.8), 5.4, "steel", shade=0, flat=True)
    for side in (-1, 1):
        g.path([(side * 5, 9, 18), (side * 12, 11, 12), (side * 15.5, 13, 4)], 0.8, "leather_dark", square=True)
    # The dish helmet: a low crown of plates, a wide brim, netting, rags tied in, a band round it.
    g.rounded((0, 27, 1), (17.4, 12.0, 16.4), "helmet", power=2.4, keep=lambda x, y, z: y > 26)
    for x in range(-25, 25):
        for z in range(-24, 26):
            d = (abs(x + 0.5) / 24.5) ** 4 + (abs(z + 0.5 - 1) / 24.0) ** 4
            if d <= 1:
                g.put((x, 26 if d < 0.6 else 25, z), "helmet")
                if d < 0.6:
                    g.put((x, 27, z), "helmet")
    band(g, 28, 31, 16.8, 15.8, "leather_dark", out=0.8)
    for (x, y, z), (material, _) in list(g.cells.items()):
        if material == "helmet" and y > 31 and (x + y + z) % 5 == 0 and (x - y + z) % 5 == 0:
            g.put((x, y, z), "scrim", 0)
        elif material == "helmet" and hashed(x // 3, y // 2, z // 3, 77) < 0.08:
            g.put((x, y, z), "rust")
    for i in range(14):
        a = hashed(i, 5) * math.tau
        r = 6 + hashed(i, 6) * 9
        x, z = int(math.cos(a) * r), int(math.sin(a) * r)
        top = max((y for (cx, y, cz) in g.cells if cx == x and cz == z), default=None)
        if top is not None:
            g.box((x - 1, top, z - 1), (x + 2, top + 2, z + 2), "scrim")
    goggles(g, 31, 15.4)
    g.path([(-16.4, 25, 4), (-15.4, 8, 7), (-10, 1, 9)], 0.9, "leather")
    g.path([(16.4, 25, 4), (15.4, 8, 7), (10, 1, 9)], 0.9, "leather")


def scavenger_head(model):
    g = grid(model, "Head")
    head_shape(g)
    face(g, "mask")
    # A steel helmet cut high over the ears, plates and rails on its sides, a headset's big ear cups over it.
    def shell(x, y, z):
        return (y > 22) & ((np.abs(x) / 19.6) ** 2.6 + (np.abs(y - 22) / 13.0) ** 2.6 + (np.abs(z - 1) / 17.8) ** 2.6
                           <= 1)
    g.fill(shell, ((-21, 22, -18), (21, 36, 20)), "helmet")
    g.fill(lambda x, y, z: (y > 15) & (y <= 22) & ((np.abs(x) / 19.6) ** 2.6 + (np.abs(z - 1) / 17.8) ** 2.6 <= 1) &
           ~((z > 4) & (np.abs(x) < 16.5)), ((-21, 15, -18), (21, 23, 20)), "helmet")
    for side in (-1, 1):
        g.box((side * 18.8 - 1.4, 22, -9), (side * 18.8 + 1.4, 25, 9), "steel", round_edges=0.5)
        g.box((side * 15 - 3, 28, -6), (side * 15 + 3, 32, 2), "helmet", round_edges=0.6)
        g.rounded((side * 19.6, 15, 1.5), (3.8, 7.4, 6.8), "steel", power=2.6)
        g.rounded((side * 22.0, 15, 1.5), (1.8, 5.4, 5.0), "tan_dark", power=2.4)
        g.rod((side * 19.4, 21, 1.5), (side * 18.4, 28, 1.5), 1.2, "leather_dark")
    g.box((-15, 33, -8), (15, 34.6, 8), "helmet", shade=3)
    g.path([(17, 11, 5), (14, 8, 12), (8, 6, 17.6)], 0.9, "blued")
    g.ball((7.0, 6.0, 18.0), (1.8, 1.8, 1.8), "blued")
    for (x, y, z), (material, _) in list(g.cells.items()):
        if material == "helmet" and hashed(x // 3, y // 3, z // 3, 41) < 0.07:
            g.put((x, y, z), "rust")
    goggles(g, 26, 16.2)
    band(g, 27, 29.5, 19.2, 17.4, "leather_dark", out=0.6)


# Gear on the back ------------------------------------------------------------------------------------------------


def rifleman_gear(model):
    chest = grid(model, "Chest")
    chest.box((-12, -4, -21), (12, 14, -14), "tan", round_edges=1.2)
    chest.box((-12, 9, -22), (12, 14, -14), "tan_dark", round_edges=0.8)
    for x in (-7, 5):
        chest.box((x, -2, -22.6), (x + 2, 12, -21), "leather")
        chest.box((x, 3, -23), (x + 2, 5, -22.4), "brass", shade=1)
    pouch(chest, (-6, -3, -24), (6, 5, -21))
    chest.rod((-15, 17, -17), (15, 17, -17), 4.6, "rag")
    for x in (-10, 8):
        chest.rod((x, 17, -17), (x + 1.6, 17, -17), 5.0, "leather_dark", flat=True)


def scavenger_gear(model):
    chest = grid(model, "Chest")
    chest.box((-12, -5, -22), (12, 15, -14), "tan_dark", round_edges=1.2)
    chest.box((-10, -2, -23), (10, 11, -21.6), "steel")
    for x in (-7, -2, 3):
        chest.ball((x + 0.5, 7.5, -23.2), (1.2, 1.2, 1.0), "brass", shade=3)
    chest.box((-8, 0, -23.6), (8, 3, -22.8), "blued")
    for x, height in ((-9, 30), (9, 22)):
        chest.box((x - 0.6, 15, -18.6), (x + 0.6, 15 + height, -17.4), "blued")
        chest.rod((x, 15, -18), (x, 18, -18), 1.8, "blued")
    chest.path([(13, 19, 11), (-2, 5, 18.5), (-15, -5, 13)], 1.6, "leather", square=True)
    for t in np.linspace(0.08, 0.92, 10):
        x, y = 13 - 28 * t, 19 - 24 * t
        spot = front_of(chest, int(x), int(y))
        if spot is not None:
            chest.box((int(x), int(y) - 1, spot + 1), (int(x) + 1, int(y) + 2, spot + 2), "brass", shade=2)


# Guns -------------------------------------------------------------------------------------------------------------


def rifle(g):
    """A bolt-action rifle along +Z, chunky as the style wants, the middle of its wrist at the origin. Gives where
    each hand holds it: for each, the grip's axis, the way from the palm to it, its middle and how thick it is."""
    g.panels = True
    wrist_axis = np.array((0.0, 0.77, 0.64))
    g.rod(-wrist_axis * 5.5, wrist_axis * 5.0, GRIP_RADIUS, "wood")
    # A short stock that ends at the shoulder, a brass plate on its end.
    for z in range(-24, -3):
        t = (z + 24) / 21
        g.box((-2.8, -10.5 + t * 7.5, z), (2.8, 4.4 - t * 0.4, z + 1), "wood", shade=2 if z % 7 else 1)
    g.box((-3.0, -11.0, -26), (3.0, 4.8, -24), "brass", shade=1)
    # The receiver, magazine, trigger guard and trigger, the bolt and its handle out to the right.
    g.box((-3.0, 2.0, -2), (3.0, 8.6, 18), "blued", round_edges=0.6)
    g.box((-2.6, -5.6, 6), (2.6, 2.0, 14), "blued", round_edges=0.5)
    g.path([(0, 1.0, 1.2), (0, -4.0, 2.8), (0, -4.0, 6.0)], 0.9, "blued")
    g.box((-0.8, -3.0, 3.6), (0.8, 1.0, 5.0), "steel", shade=3)
    g.rod((0, 8.4, -2), (0, 8.4, 9), 2.0, "steel")
    g.path([(1.6, 8.0, 1.5), (5.6, 5.8, 2.0)], 1.0, "steel")
    g.ball((6.2, 5.4, 2.2), (1.8, 1.8, 1.8), "steel", shade=3)
    # The forestock, the handguard over the barrel, the barrel, its nose cap, sights and bands.
    g.rod((0, 2.4, 16), (0, 2.4, 50), 3.0, "wood", flat=True)
    g.box((-2.4, 5.4, 18), (2.4, 8.4, 44), "wood", round_edges=0.5)
    g.rod((0, 6.8, 18), (0, 6.8, 64), 1.6, "blued")
    g.box((-2.8, -1.2, 50), (2.8, 9.4, 57), "blued", round_edges=0.5)
    g.box((-1.0, 9.4, 54), (1.0, 12.0, 56), "blued")
    g.box((-2.4, 8.4, 19), (2.4, 11.4, 23), "blued", round_edges=0.4)
    for z in (34, 44):
        g.box((-3.6, -1.0, z), (3.6, 9.2, z + 1.6), "brass", shade=0)
    # Rags wound round the forestock where the other hand holds it, and a sling under it all.
    for i, z in enumerate(range(15, 27, 2)):
        g.rod((0, 2.4, z), (0, 2.4, z + 2), 3.4, "rag", shade=0 if i % 2 else 2, flat=True)
    g.path([(0, -9.5, -20), (0, -12, -8), (0, -10, 10), (0, -3, 34)], 0.8, "leather", square=True)
    return {
        "right": (wrist_axis, (-0.85, -0.5, 0.0), np.zeros(3), GRIP_RADIUS),
        "carry": ((0.0, 0.0, 1.0), (0.0, 1.0, 0.0), np.array((0.0, 2.4, 19.0)), 3.4),
        "aim": ((0.0, 0.0, 1.0), (0.0, 1.0, 0.0), np.array((0.0, 2.4, 24.0)), 3.4),
        "stock": np.array((0.0, -3.2, -25.0)),
    }


def smg(g):
    """A drum-fed submachine gun along +Z, the middle of its pistol grip at the origin, a vertical grip under its
    barrel for the other hand. Gives where each hand holds it, as `rifle` does."""
    g.panels = True
    grip_axis = np.array((0.0, 0.8, 0.6))
    g.rod(-grip_axis * 5.5, grip_axis * 5.0, GRIP_RADIUS, "wood")
    g.box((-2.8, -7.0, -3.4), (2.8, -5.0, 1.0), "wood", round_edges=1)
    # The receiver, its top cover, sights and cocking knob, and a wooden stock behind.
    g.box((-3.6, 4.8, -4), (3.6, 12.0, 24), "blued", round_edges=0.6)
    g.box((-2.8, 12.0, 2), (2.8, 13.6, 16), "steel", round_edges=0.4)
    g.box((-1.0, 12.8, -1), (1.0, 15.2, 1.6), "blued")
    g.box((3.6, 8.0, 10), (5.6, 10.0, 12.4), "steel", shade=3)
    for z in range(-22, -4):
        t = (z + 22) / 18
        g.box((-2.8, 4.4 - (1 - t) * 7.0, z), (2.8, 11.2, z + 1), "wood", shade=2 if z % 6 else 1)
    g.box((-3.0, -3.2, -24), (3.0, 11.6, -22), "steel", shade=0)
    g.path([(0, 4.8, 1.6), (0, 1.2, 3.2), (0, 1.2, 7.6), (0, 4.8, 8.4)], 0.9, "blued")
    g.box((-0.8, 2.2, 4.8), (0.8, 6.0, 6.4), "steel", shade=3)
    # The barrel with its cooling fins, a compensator and the foresight.
    g.rod((0, 8.4, 24), (0, 8.4, 42), 2.6, "blued", flat=True)
    for z in range(25, 41, 3):
        g.rod((0, 8.4, z), (0, 8.4, z + 1.2), 3.6, "steel", shade=0, flat=True)
    g.box((-2.4, 6.0, 42), (2.4, 10.8, 46), "blued", round_edges=0.4)
    g.box((-1.0, 10.8, 43), (1.0, 12.8, 45), "blued")
    # The drum before the trigger, a ring and hub on its face, rust at its rim.
    g.rod((-3.8, -2.0, 13), (3.8, -2.0, 13), 7.2, "blued", flat=True)
    g.rod((-4.2, -2.0, 13), (4.2, -2.0, 13), 2.4, "steel", flat=True)
    for side in (-1, 1):
        g.fill(lambda x, y, z, side=side: (np.hypot(y + 2.0, z - 13) > 4.6) & (np.hypot(y + 2.0, z - 13) < 5.6) &
               (x * side > 3.4) & (x * side < 4.2), ((-5, -10, 5), (5, 6, 21)), "steel", shade=3)
    for (x, y, z), (material, _) in list(g.cells.items()):
        if material == "blued" and abs(x + 0.5) >= 3.4 and hashed(x, y, z, 31) < 0.12:
            g.put((x, y, z), "rust")
    fore_axis = np.array((0.0, 0.98, 0.18))
    top = np.array((0, 6.0, 24.0))
    g.rod(top - fore_axis * 11.0, top, GRIP_RADIUS, "wood")
    middle = top - fore_axis * 6.0
    return {
        "right": (grip_axis, (-0.85, -0.5, 0.0), np.zeros(3), GRIP_RADIUS),
        "carry": (fore_axis, (1.0, 0.0, 0.0), middle, GRIP_RADIUS),
        "aim": (fore_axis, (1.0, 0.0, 0.0), middle, GRIP_RADIUS),
        "stock": np.array((0.0, 4.0, -23.0)),
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
    "rifleman": ((-0.55, -0.62, 0.42), (0.15, 1.0, 0.35), (6.0, 63.0, 21.0)),
    "scavenger": ((-0.55, -0.62, 0.42), (0.15, 1.0, 0.35), (6.0, 63.0, 21.0)),
}
# Which way each elbow points, the right's then the left's, at rest and aiming.
POLES = {"carry": ((1.0, -0.5, -0.6), (-0.6, -1.0, -0.2)), "aim": ((1.0, 0.0, -0.4), (-0.3, -1.0, -0.2))}
# Aiming: how far the hips turn side on, and where in the chest the stock sits.
AIM = {"rifleman": (60.0, (18.0, 6.0, 12.0)), "scavenger": (60.0, (18.0, 14.0, 12.0))}


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
    return gun_r, pocket - gun_r @ grips["stock"] + np.array((0.0, 0.0, -kick * 2.4))


def make(kind):
    model = Model(palette(kind), pixels_per_block=PIXELS)
    skeleton(model)
    body(model, kind)
    rig(model, kind)
    if kind == "rifleman":
        rifleman_head(model)
        rifleman_gear(model)
    else:
        scavenger_head(model)
        scavenger_gear(model)
    gun = model.bone("Gun", "Chest")
    grips = rifle(gun.grid) if kind == "rifleman" else smg(gun.grid)
    for name, held in (("Left", "carry"), ("Right", "right")):
        hand(model, name, "glove", fingertips="skin" if kind == "scavenger" else None, radius=grips[held][3])

    # At rest the arms hold the gun low across the body: that pose's turnings are the bones' rest.
    gun_r, gun_t = carry(kind)
    # The left shoulder reaches forward a little, as one does to hold a gun across the body.
    reach = {"LeftShoulder": ((0.0, 25.0, 0.0), (0, 0, 0))}
    for name, (rotation, move) in hold(model, grips, gun_r, gun_t, reach, "carry", f"{kind} carrying").items():
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
