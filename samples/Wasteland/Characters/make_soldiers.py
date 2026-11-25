#!/usr/bin/env python3
"""Makes the Wasteland soldiers as box models of small cubes, with their bones and aim clips.

    python3 make_soldiers.py              # writes Rifleman.boxmodel and Scavenger.boxmodel beside this file
    python3 make_soldiers.py --preview D  # and quick pictures of each, at rest and aiming, into folder D

Each soldier is a chibi figure: a big helmeted head, a short body, stubby arms and legs on upper and lower bones,
and a gun on a bone of its own held across the chest. Mob Motion walks, runs and idles them from how they move;
the arms are built holding the gun, so they swing only a little. Aiming is a clip in each model ("Aim"), with
"Fire" for the kick of a shot. The models can be changed in the Model window afterwards, though running this
again makes them afresh.
"""

import argparse
import math
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from voxkit import Model, Palette, basis, hashed, preview, reach, rot_x, rot_y, write_model  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))

# Where the parts meet, in cubes (32 to a metre): feet on 0, hips at 14, shoulders at 28, the neck at 30.
HIP = 14
LEG = 7
NECK = 16
SHOULDER = (10.5, 14.0)  # out from the middle and up from the hips
UPPER_ARM = 7
HAND = (0.0, -6.0, 0.0)  # in the lower arm, from the elbow


def palette(kind):
    p = Palette()
    p.add("skin", "e8b48e", warm=0.02)
    p.add("eye", "2b1d1a", shades=(1.0,))
    p.add("eye_light", "f4efe6", shades=(1.0,))
    p.add("eye_iris", "5e3424", shades=(0.9, 1.0))
    p.add("blush", "e39a84", shades=(1.0,))
    p.add("leather", "6b4428", warm=0.04)
    p.add("leather_dark", "3e2a1c", warm=0.03)
    p.add("boot", "2b2420")
    p.add("sole", "1c1816", shades=(0.9, 1.0))
    p.add("brass", "b08a3e", shades=(0.8, 1.0, 1.15))
    p.add("gunmetal", "3a3d40")
    p.add("wood", "7a4a2a", warm=0.05)
    p.add("canvas", "9b8a62")
    p.add("rag", "b8a684")
    p.add("rust", "8a4a26", warm=0.05)
    p.add("glass", "4a6e6c", shades=(0.8, 1.0))
    p.add("glint", "cfe6e0", shades=(1.0,))
    if kind == "rifleman":
        p.add("coat", "6e6a48", warm=0.03)  # a greatcoat faded to olive
        p.add("coat_dark", "4e4b34", warm=0.03)
        p.add("trousers", "5e5640")
        p.add("puttee", "8a7c58")
        p.add("helmet", "5c5e44", warm=0.02)
        p.add("mask", "3d4136")
        p.add("hood", "8e826a")
        p.add("patch_a", "7a3e32")  # patches from other coats
        p.add("patch_b", "4a5662")
        p.add("scrim", "7d7a52")
    else:
        p.add("coat", "5a4a3c", warm=0.03)  # a trench coat gone brown with mud
        p.add("coat_dark", "3e3229", warm=0.03)
        p.add("trousers", "4c4f50")
        p.add("puttee", "6c6a5c")
        p.add("helmet", "5b5f5e", warm=0.02)
        p.add("scarf", "8c3a30", warm=0.03)
        p.add("scarf_light", "5a2620")
        p.add("hair", "3a2a20")
        p.add("plate", "7c7c76")
        p.add("patch_a", "5e6a46")
        p.add("patch_b", "8a7550")
    return p


# The body, legs and arms both soldiers share ----------------------------------------------------------------------


def torn_hem(grid, y_bottom, bounds_x, bounds_z, seed, deep=2):
    """Bites out of a hem: cubes taken from its lowest rows here and there, as a coat worn to rags."""
    for (x, y, z) in list(grid.cells):
        if y < y_bottom + deep and grid.cells[(x, y, z)][0] in ("coat", "coat_dark"):
            bite = hashed(x // 2, z // 2, seed)
            if (y - y_bottom) < bite * (deep + 1.4) - 0.6:
                grid.cells.pop((x, y, z))


def patch(grid, lo, hi, material, normal):
    """A square of another cloth over a coat, stitched round with darker cubes, standing out a little."""
    axis = int(np.argmax(np.abs(normal)))
    for x in range(lo[0], hi[0]):
        for y in range(lo[1], hi[1]):
            for z in range(lo[2], hi[2]):
                if (x, y, z) not in grid.cells:
                    continue
                edge = sum(1 for a, v in ((0, x), (1, y), (2, z)) if a != axis and (v == lo[a] or v == hi[a] - 1))
                if edge and hashed(x, y, z, 3) < 0.5:
                    grid.put((x, y, z), "coat_dark", 0)
                else:
                    grid.put((x, y, z), material)


def build_body(model, kind):
    body = model.bone("Body", pivot=(0, HIP, 0), role="body")
    g = body.grid
    # The coat over the chest, its corners worn round, and its skirt to the hips.
    g.box((-8, 0, -5), (8, NECK, 5), "coat", round_edges=1)
    g.box((-8, -1, -5), (8, 2, 5), "coat", round_edges=1)
    # Collar turned up round the neck.
    g.box((-6, NECK - 1, -4), (6, NECK + 1, 3), "coat_dark", round_edges=1)
    g.remove(lambda x, y, z: abs(x) < 4 and z > -2 and y > NECK - 1, ((-6, NECK - 1, -4), (6, NECK + 1, 3)))
    # The front's edge, double-breasted with brass buttons.
    g.box((-1, 0, 5), (1, NECK - 1, 6), "coat_dark")
    for y in (4, 8, 12):
        for x in (-4, 3):
            g.put((x, y, 5), "brass", 2)
    # The belt, with a buckle.
    g.box((-8.5, 2, -5.5), (8.5, 4, 5.5), "leather")
    g.box((-1.5, 1.5, 5.5), (1.5, 4.5, 6.5), "brass")
    g.box((-0.5, 2.5, 5.5), (0.5, 3.5, 6.5), "leather_dark")
    # Shoulder straps: braces of webbing down the front and crossing at the back.
    for side in (-1, 1):
        g.rod((side * 5, NECK, 5.2), (side * 4.5, 4, 5.6), 0.7, "leather", square=True)
        g.rod((side * 5, NECK, -5.2), (-side * 5, 4, -5.6), 0.7, "leather", square=True)
        g.rod((side * 5, NECK + 0.2, -5), (side * 5, NECK + 0.2, 5), 0.7, "leather", square=True)
    # Ammunition pouches on the belt, flaps darker, a brass stud each.
    for x0 in (-7.5, -3.5, 1.5, 5.5) if kind == "rifleman" else (-7.5, 4.5):
        g.box((x0, 3, 5.5), (x0 + 3.5, 8, 7.5), "leather", round_edges=0)
        g.box((x0, 7, 5.5), (x0 + 3.5, 8.5, 8), "leather_dark")
        g.put((int(x0 + 1.5), 7, 7), "brass", 1)
    # Patches.
    patch(g, (2, 9, 4), (6, 13, 6), "patch_a", (0, 0, 1))
    patch(g, (-7, 5, -6), (-2, 9, -4), "patch_b", (0, 0, -1))
    torn_hem(g, -1, (-8, 8), (-5, 5), 11)
    return body


def build_legs(model):
    for side, name in ((1, "Right"), (-1, "Left")):
        leg = model.bone(f"{name}Leg", parent="Body", pivot=(side * 4.0, 0, 0), role="leg")
        g = leg.grid
        g.box((-3.5, -LEG, -3.5), (3.5, 0, 3.5), "trousers", round_edges=1)
        # The coat's tail over the thigh, ragged at its foot, so it swings with the leg.
        g.box((-4, -5, -4.5), (4, 0, 4.5), "coat", round_edges=1)
        g.remove(lambda x, y, z: x * side < -2.5 and z > 0, ((-4, -5, -4.5), (4, 0, 4.5)))  # the slit inside
        torn_hem(g, -5, (-4, 4), (-4.5, 4.5), 23 + side, deep=2)
        lower = model.bone(f"{name}LegLower", parent=f"{name}Leg", pivot=(0, -LEG, 0), role="leg")
        g = lower.grid
        # Puttees wound round the shin, each turn a little lower at the front.
        g.box((-3, -4, -3), (3, 0, 3), "puttee")
        for (x, y, z) in list(g.cells):
            if (y + (z > 0)) % 2 == 0:
                g.put((x, y, z), "puttee", 0)
        # The boot: chunky, its toe long, its sole a darker row, laces up the front.
        g.box((-3.5, -LEG, -4), (3.5, -3.5, 4), "boot", round_edges=1)
        g.box((-3.5, -LEG, 0), (3.5, -4.5, 6.5), "boot", round_edges=1)
        g.box((-3.5, -LEG, -4), (3.5, -LEG + 1, 6.5), "sole")
        for y in (-5, -4):
            g.put((0, y, 4), "leather_dark", 3)
            g.put((-1, y, 4), "leather_dark", 3)


def build_arms(model, kind):
    """Arms that hang straight at rest in their bones; the bones are turned to hold the gun afterwards."""
    for side, name in ((1, "Right"), (-1, "Left")):
        arm = model.bone(f"{name}Arm", parent="Body", pivot=(side * SHOULDER[0], SHOULDER[1], 0), role="arm")
        g = arm.grid
        g.box((-3, -UPPER_ARM, -3), (3, 1, 3), "coat", round_edges=1)
        # A shoulder strap, buttoned.
        g.box((-2, 1, -2), (2, 2, 2), "coat_dark")
        g.put((0, 1, 1), "brass", 2)
        if kind == "rifleman" and side < 0:
            patch(g, (-4, -5, -2), (-2, -1, 2), "patch_b", (-1, 0, 0))
        lower = model.bone(f"{name}ArmLower", parent=f"{name}Arm", pivot=(0, -UPPER_ARM, 0))
        g = lower.grid
        g.box((-3, -3, -3), (3, 0.5, 3), "coat", round_edges=1)
        g.box((-3.2, -3.5, -3.2), (3.2, -2.5, 3.2), "coat_dark")  # the cuff
        # Hands in gloves, or wound in rags.
        hand = "leather" if kind == "rifleman" else "rag"
        g.box((-2.5, -8, -2.5), (2.5, -3.5, 2.5), hand, round_edges=1)
        if kind == "scavenger":
            for (x, y, z) in list(g.cells):
                if g.cells[(x, y, z)][0] == "rag" and (y + x) % 3 == 0:
                    g.put((x, y, z), "skin")


# Heads -----------------------------------------------------------------------------------------------------------


def head_shape(g, material):
    """A big head, a cube with its corners rounded away, as chibi figures have."""
    def inside(x, y, z):
        return (abs(x) / 9.5) ** 3 + (abs(y - 9.5) / 8.5) ** 3 + (abs(z) / 8.5) ** 3 <= 1.0
    g.fill(inside, ((-10, 0, -9), (10, 19, 9)), material)


def front(g, x, y):
    """The foremost cube of the head at `x`, `y`."""
    zs = [z for (cx, cy, z) in g.cells if cx == x and cy == y]
    return max(zs) if zs else None


def paint_front(g, x, y, material, out=0):
    z = front(g, x, y)
    if z is not None:
        g.put((x, y, z + out), material)


def eyes(g):
    """Big eyes, dark lashes over a red-brown iris lit at the top, as the chibi style has them."""
    for side in (-1, 1):
        inner, outer = (1, 4) if side > 0 else (-2, -5)
        step = 1 if side > 0 else -1
        for x in range(inner, outer + step, step):
            for y in range(6, 10):
                paint_front(g, x, y, "eye_iris")
            paint_front(g, x, 10, "eye")
        paint_front(g, outer + step, 10, "eye")
        paint_front(g, outer + step, 9, "eye")
        paint_front(g, inner + step, 9, "eye_light")
        paint_front(g, outer + step * 2, 5, "blush")
        paint_front(g, outer + step, 5, "blush")


def build_rifleman_head(model):
    head = model.bone("Head", parent="Body", pivot=(0, NECK, 0), role="head")
    g = head.grid
    # A cloth hood under the helmet, round the back and sides.
    head_shape(g, "hood")
    # The gas mask over the face: rubber standing out a cube, two round glass eyes in steel rings, and a snout
    # down to a filter tin.
    def mask(x, y, z):
        return z > 3 and y < 14.5 and (x / 10) ** 4 + ((y - 9.5) / 9) ** 4 + (z / 9.5) ** 4 <= 1.0
    g.fill(mask, ((-10, 0, 0), (10, 15, 10)), "mask")
    for side in (-1, 1):
        cx = side * 4.0
        g.rod((cx, 9.5, 7), (cx, 9.5, 10), 2.9, "brass", flat=True)
        g.rod((cx, 9.5, 7), (cx, 9.5, 10.6), 2.2, "glass", flat=True)
        g.put((int(math.floor(cx)) - side, 10, 10), "glint")
        g.put((int(math.floor(cx)) - side, 11, 10), "glint")
    g.rod((0, 4.5, 8), (0, 3.5, 11), 2.0, "mask")
    g.rod((0, 3.5, 10.5), (0, 3.5, 13.5), 2.5, "gunmetal", flat=True)
    g.rod((0, 3.5, 12.5), (0, 3.5, 14), 1.3, "rust", flat=True)
    # The mask's straps across the hood.
    for y in (6, 12):
        g.box((-9.5, y, -8.5), (9.5, y + 1, 7), "leather_dark", only=("hood",))
    # The helmet: a shallow dish, its crown low and its brim wide all round, with rags tied over it.
    g.ball((0, 15, 0), (10.5, 6.5, 10), "helmet", keep=lambda x, y, z: y > 15)
    for x in range(-15, 16):
        for z in range(-15, 16):
            d = math.hypot(x + 0.5, (z + 0.5) * 1.05)
            if d <= 14.2:
                y = 15 if d < 12 else 14  # the brim turns down at its rim
                g.put((x, y, z), "helmet")
    g.box((-11, 16, -11), (11, 17, 11), "rust", only=("helmet",))  # a ring of rust where the brim meets
    for i in range(26):
        # Strips of scrim cloth tied to the helmet, hanging past the brim here and there.
        a = hashed(i, 5) * math.tau
        r = 6 + hashed(i, 6) * 6
        x, z = int(math.cos(a) * r), int(math.sin(a) * r)
        for y in range(20, 14, -1):
            if (x, y, z) in g.cells:
                g.put((x, y + 1, z), "scrim")
                if r > 10 and hashed(i, 9) < 0.5:
                    for down in range(1, 4):
                        g.put((x, 14 - down, z), "scrim")
                break
    for (x, y, z) in list(g.cells):
        if g.cells[(x, y, z)][0] == "helmet" and hashed(x // 2, y, z // 2, 77) < 0.12:
            g.put((x, y, z), "rust")
    # The chin strap.
    for side in (-1, 1):
        g.box((side * 9 - (1 if side > 0 else 0), 1, 2), (side * 9 + (1 if side < 0 else 0), 14, 4), "leather")
    return head


def build_scavenger_head(model):
    head = model.bone("Head", parent="Body", pivot=(0, NECK, 0), role="head")
    g = head.grid
    head_shape(g, "skin")
    # Hair over the back and sides, short, and a fringe under the helmet.
    g.box((-10, 4, -9), (10, 19, 2), "hair", only=("skin",))
    g.box((-10, 13, -9), (10, 19, 9), "hair", only=("skin",))
    for x in range(-8, 8):
        if hashed(x, 3) < 0.5:
            paint_front(g, x, 12, "hair")
    eyes(g)
    # A rag scarf over the nose and mouth, knotted at the back, its tails hanging.
    def scarf(x, y, z):
        return y < 6.5 and (abs(x) / 10.6) ** 3 + (abs(y - 9.5) / 9.6) ** 3 + (abs(z) / 9.8) ** 3 <= 1.0
    g.fill(scarf, ((-11, -1, -10), (11, 7, 11)), "scarf")
    # Its point hanging under the chin.
    for y in range(-3, 1):
        for x in range(-(y + 4), y + 5):
            g.put((x, y, 8), "scarf")
            g.put((x, y, 7), "scarf")
    for (x, y, z) in list(g.cells):
        # A worn check: darker bands across and down, broken where the threads have gone.
        if g.cells[(x, y, z)][0] == "scarf" and ((x + z) % 6 == 0 or y % 5 == 1) and hashed(x, y, z, 8) < 0.7:
            g.put((x, y, z), "scarf_light")
    g.box((-1.5, 1, -10.5), (1.5, 4, -8.5), "scarf")
    g.rod((-1, 2, -10), (-2, -4, -11), 0.9, "scarf", square=True)
    g.rod((1, 2, -10), (2.5, -3, -11), 0.9, "scarf_light", square=True)
    # The helmet: a deep steel bowl, its skirt flaring out over the ears and the neck, a short peak in front.
    def bowl(x, y, z):
        return y > 13 and (x / 10.8) ** 2 + ((y - 13) / 8.6) ** 2 + (z / 10.4) ** 2 <= 1
    g.fill(bowl, ((-11, 13, -11), (11, 22, 11)), "helmet")
    for x in range(-13, 13):
        for z in range(-13, 12):
            r = math.hypot((x + 0.5) / 10.5, (z + 0.5) / 10.2)
            if 0.82 < r <= 1.17 and (z < 7 or abs(x) > 7):
                # The skirt steps down and out towards the back and sides.
                drop = 0 if z > 5 else 3 if z < -3 else 1 + (abs(x) > 6)
                for y in range(13 - drop, 15):
                    g.put((x, y, z), "helmet")
    g.box((-7, 13, 8), (7, 14, 11), "helmet")  # the peak
    # Goggles pushed up on the helmet: a leather strap round it and two round lenses on the front.
    g.box((-11, 16, -11), (11, 17.5, 11), "leather_dark", only=("helmet",))
    for side in (-1, 1):
        cx = side * 3.6
        g.rod((cx, 17, 9), (cx, 17, 12), 2.7, "brass", flat=True)
        g.rod((cx, 17, 10), (cx, 17, 12.6), 1.9, "glass", flat=True)
        g.put((int(math.floor(cx)), 18, 12), "glint")
    g.box((-0.5, 16.5, 10), (0.5, 17.5, 11.5), "leather_dark")
    # Dents, scratches and rust.
    for (x, y, z) in list(g.cells):
        if g.cells[(x, y, z)][0] == "helmet":
            h = hashed(x // 2, y // 2, z // 2, 41)
            if h < 0.1:
                g.put((x, y, z), "rust")
            elif h > 0.96:
                g.put((x, y, z), "plate", 3)
    return head


# Gear ------------------------------------------------------------------------------------------------------------


def rifleman_gear(model):
    g = model.by_name("Body").grid
    # A blanket rolled over the back of the shoulders, tied with straps.
    g.rod((-8.5, NECK - 1, -7.5), (8.5, NECK - 1, -7.5), 2.4, "canvas")
    for x in (-5, 4):
        g.box((x, NECK - 4, -10), (x + 1, NECK + 2, -5), "leather_dark", only=("canvas",))
    # A pack on the back: canvas, flap and buckles.
    g.box((-6.5, 3, -9), (6.5, 13, -5), "canvas", round_edges=1)
    g.box((-6.5, 10, -9.5), (6.5, 13, -5), "canvas", shade=0)
    for x in (-4, 3):
        g.box((x, 7, -10), (x + 1, 12, -9), "leather")
        g.put((x, 7, -10), "brass", 1)
    # A mess tin strapped to the pack.
    g.box((-3, 4, -10.5), (3, 8, -9), "gunmetal", round_edges=0)
    # The mask's bag on the left hip and a bayonet's scabbard on the right.
    g.box((-11.5, -2, -1), (-8, 5, 4), "canvas", round_edges=1)
    g.box((-11.5, 3, -1), (-8, 5, 4), "canvas", shade=0)
    g.rod((8.8, 2, -2), (9.6, -7, -4), 0.8, "leather_dark", square=True)
    g.box((8, 2, -3.5), (10, 4, -0.5), "leather")


def scavenger_gear(model):
    g = model.by_name("Body").grid
    # Scrap plates strapped over the chest: a sheet of rusted steel riveted on, a smaller one below.
    g.box((-6, 7, 6), (5, 14, 7), "plate", round_edges=0)
    g.box((-5, 4, 6), (3, 7, 7), "rust")
    for (x, y, z) in list(g.cells):
        if z == 6 and g.cells[(x, y, z)][0] == "plate":
            h = hashed(x // 2, y // 2, 99)
            if h < 0.3:
                g.put((x, y, z), "rust")
            if (x in (-6, 4)) and y in (8, 12):
                g.put((x, y, z), "brass", 0)
    for y in (9, 12):
        g.box((-8.5, y, -5.5), (8.5, y + 1, 7.5), "leather_dark", only=("coat", "coat_dark", "plate", "leather"))
    # A bandolier across from the right shoulder, rounds in it.
    g.rod((6, NECK, 5.8), (-7, 2, 6.2), 1.2, "leather", square=True)
    for t in np.linspace(0.1, 0.9, 6):
        x, y = 6 + (-13) * t, NECK + (2 - NECK) * t
        g.put((int(x), int(y), 7), "brass", 2)
    # A satchel on the back and a canteen at the hip.
    g.box((-6, 4, -9.5), (5, 12, -5), "leather", round_edges=1)
    g.box((-6, 9, -10), (5, 12, -5), "leather_dark")
    g.put((-1, 9, -10), "brass", 2)
    g.rod((9.8, 0, 1.5), (9.8, 4, 1.5), 2.2, "gunmetal")
    g.rod((9.8, 0.5, 1.5), (9.8, 3.5, 1.5), 2.3, "canvas")
    g.put((10, 4, 1), "rust")
    # A pauldron made of a pan, strapped to the left shoulder.
    arm = model.by_name("LeftArm").grid
    arm.ball((-1, 0, 0), (4.2, 3.2, 4.2), "plate", keep=lambda x, y, z: y > -1)
    arm.box((-4.5, -2, -0.5), (3.5, -1, 0.5), "leather_dark")
    for (x, y, z) in list(arm.cells):
        if arm.cells[(x, y, z)][0] == "plate" and hashed(x, y, z, 5) < 0.25:
            arm.put((x, y, z), "rust")


# Guns ------------------------------------------------------------------------------------------------------------


def build_rifle(g):
    """A bolt-action rifle along +Z, the hand's grip at the origin. Returns where the other hand holds it."""
    # The stock: a wooden butt behind the grip, deeper at the heel.
    g.box((-1, -3.5, -14), (1, 1, -11), "wood")
    for z in range(-14, -1):
        depth = 3.5 - (z + 14) * 0.2
        g.box((-1, -depth, z), (1, 1, z + 1), "wood")
    g.box((-1, -4, -14.5), (1, 1.2, -13.5), "leather_dark")  # a butt plate
    # The receiver, bolt and its handle out to the right, and the trigger guard.
    g.box((-1, -0.5, -2), (1, 2, 6), "gunmetal")
    g.box((0, 2, -1), (1, 3, 3), "gunmetal")
    g.box((1, 1, 0), (3, 2, 1), "gunmetal")
    g.box((2, 0, 0), (3, 1, 1), "gunmetal", shade=3)
    g.box((-0.5, -2, 1), (0.5, -0.5, 4), "gunmetal")
    # The forestock under the barrel, wound with rags where the other hand goes.
    g.box((-1, -1, 6), (1, 1, 19), "wood")
    g.box((-1.3, -1.3, 8), (1.3, 1.3, 12), "rag")
    for z in range(8, 12):
        if z % 2:
            g.box((-1.3, -1.3, z), (1.3, 1.3, z + 1), "rag", shade=0)
    # The barrel, its sight and the bands holding it.
    g.box((-0.5, 1, 6), (0.5, 2, 26), "gunmetal")
    g.box((-0.5, 2, 24), (0.5, 3, 25), "gunmetal")
    g.box((-1, 2, 6), (1, 3, 8), "gunmetal", shade=0)
    for z in (14, 18):
        g.box((-1.2, -1.2, z), (1.2, 2.2, z + 1), "brass", shade=0)
    # The sling hanging under it.
    g.rod((0, -2, -10), (0, -4, 4), 0.4, "leather", square=True)
    g.rod((0, -4, 4), (0, -1.5, 15), 0.4, "leather", square=True)
    return np.array((0.0, -0.5, 10.0))


def build_smg(g):
    """A submachine gun along +Z, the grip at the origin, a drum under it. Returns where the other hand holds."""
    g.box((-1, -0.5, -11), (1, 1.5, -2), "wood")  # a wooden stock
    g.box((-1, -3, -12), (1, 1.5, -10), "wood")
    g.box((-0.5, -4, -1), (0.5, 0, 1), "wood")  # the pistol grip
    g.box((-1.3, -0.5, -2), (1.3, 2.5, 8), "gunmetal")
    g.box((1.3, 1, 2), (2.2, 2, 3), "gunmetal", shade=3)  # the cocking handle
    # The barrel's jacket, holes in rows.
    g.box((-1.2, -0.2, 8), (1.2, 2.2, 15), "gunmetal", shade=1)
    for z in range(9, 15, 2):
        g.put((1, 1, z), "sole")
        g.put((-2, 1, z), "sole")
    g.box((-0.5, 0.5, 15), (0.5, 1.5, 17), "gunmetal")
    # A round drum before the grip, and a foregrip wound in rags.
    g.rod((-1.6, -4, 4.5), (1.6, -4, 4.5), 3.4, "gunmetal")
    g.rod((-1.8, -4, 4.5), (1.8, -4, 4.5), 1.2, "rust")
    g.box((-0.9, -3.5, 10), (0.9, -0.2, 12), "rag")
    return np.array((0.0, -2.0, 11.0))


# Holding the gun: the arms reaching for it, at rest and in the clips ------------------------------------------------


def hold(model, gun_r, gun_t, fore, keys, tag):
    """Keys for the gun and both arms, the gun turned `gun_r` with its grip at `gun_t` in the model's space,
    the right hand on the grip and the left on the point `fore` along it."""
    placed = model.placed(keys)
    keys = dict(keys)
    keys["Gun"] = model.key_for("Gun", gun_r, gun_t, placed)
    grip = np.array(gun_t, float)
    held = grip + gun_r @ fore
    keys, miss_r = reach(model, "RightArm", "RightArmLower", HAND, grip, placed, keys, (1, -0.3, 0))
    keys, miss_l = reach(model, "LeftArm", "LeftArmLower", HAND, held, placed, keys, (-1, -0.3, 0))
    if max(miss_r, miss_l) > 1.5:
        print(f"  {tag}: the hands fall short by {miss_r:.1f} and {miss_l:.1f} cubes")
    return keys


def carry(kind):
    """How the gun is held at rest: across the chest, its muzzle up to the left, as on the march."""
    if kind == "rifleman":
        grip, forward = np.array((5.0, HIP + 4.5, 8.0)), np.array((-0.62, 0.72, 0.28))
    else:
        grip, forward = np.array((4.0, HIP + 5.0, 7.5)), np.array((-0.7, 0.5, 0.5))
    return basis(forward, (0.25, 0.25, 1.0)), grip


def aim_pose(sway=0.0, kick=0.0):
    """Keys for aiming: the body turned so the left shoulder leads and the short arms reach, the gun level before
    the chest pointing ahead, a breath's sway and a shot's kick over it."""
    turn = 36.0
    keys = {
        "Body": ((3.0 + kick * 3.0, turn, 0.0), (0, -0.5, 0)),
        "Head": ((5.0 - kick * 4.0, -turn + 6.0, 2.0), (0, 0, 0.5)),
        "RightLeg": ((-8.0, -turn * 0.7, 4.0), (0, 0, 0)),
        "LeftLeg": ((12.0, -turn * 0.7, -3.0), (0, 0, 0)),
        "RightLegLower": ((10.0, 0, 0), (0, 0, 0)),
        "LeftLegLower": ((5.0, 0, 0), (0, 0, 0)),
    }
    up = math.radians(-1.5 + sway * 1.2 + kick * 10.0)
    side = math.radians(sway * 0.8)
    gun_r = rot_y(side) @ rot_x(-up)
    grip = np.array((2.5, HIP + 10.5 + sway * 0.3, 6.5 - kick * 2.0))
    return keys, gun_r, grip


def make(kind):
    model = Model(palette(kind), pixels_per_block=32.0)
    build_body(model, kind)
    build_legs(model)
    if kind == "rifleman":
        build_rifleman_head(model)
    else:
        build_scavenger_head(model)
    build_arms(model, kind)
    if kind == "rifleman":
        rifleman_gear(model)
    else:
        scavenger_gear(model)
    gun = model.bone("Gun", parent="Body")
    fore = build_rifle(gun.grid) if kind == "rifleman" else build_smg(gun.grid)
    # Aiming, the other hand holds nearer the body: the arms are short.
    aim_fore = np.array((0.0, -0.5, 7.0)) if kind == "rifleman" else np.array((0.0, -2.0, 8.0))

    # The arms at rest are those holding the gun across the chest: their rest turning is that pose's.
    gun_r, grip = carry(kind)
    rest = hold(model, gun_r, grip, fore, {}, f"{kind} carrying")
    for bone in model.bones:
        if bone.name in rest:
            rotation, move = rest[bone.name]
            bone.rotation = [round(v, 3) for v in rotation]
            bone.pivot = bone.pivot + np.array(move)

    # Aiming, a breath in and out over two seconds; and a shot's kick.
    aim = []
    for t, sway in ((0.0, 0.0), (0.5, 1.0), (1.0, 0.0), (1.5, -1.0)):
        keys, gun_r, grip = aim_pose(sway=sway)
        aim.append((t, hold(model, gun_r, grip, aim_fore, keys, f"{kind} aiming")))
    fire = []
    for t, kick in ((0.0, 0.0), (0.05, 1.0), (0.2, 0.35), (0.4, 0.0)):
        keys, gun_r, grip = aim_pose(kick=kick)
        fire.append((t, hold(model, gun_r, grip, aim_fore, keys, f"{kind} firing")))
    model.clips.append(clip("Aim", 2.0, True, aim))
    model.clips.append(clip("Fire", 0.4, False, fire))
    return model, aim[0][1]


def clip(name, length, looping, timed):
    bones = {}
    names = sorted({bone for _, keys in timed for bone in keys})
    for bone in names:
        keyed = []
        for t, keys in timed:
            rotation, move = keys.get(bone, ((0, 0, 0), (0, 0, 0)))
            keyed.append((t, [round(v, 2) for v in rotation], [round(float(v), 2) for v in move]))
        bones[bone] = keyed
    return {"name": name, "length": length, "looping": looping, "bones": bones}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--preview", help="a folder for quick pictures of each soldier")
    args = parser.parse_args()
    for kind, file in (("rifleman", "Rifleman"), ("scavenger", "Scavenger")):
        model, aiming = make(kind)
        write_model(model, os.path.join(HERE, f"{file}.boxmodel"))
        cubes = sum(len(bone.grid.cells) for bone in model.bones)
        print(f"{file}: {cubes} cubes, {len(model.palette.colors)} colours")
        if args.preview:
            os.makedirs(args.preview, exist_ok=True)
            for yaw, view in ((30, "front"), (150, "back")):
                preview(model, os.path.join(args.preview, f"{file}-{view}.png"), yaw=yaw)
            preview(model, os.path.join(args.preview, f"{file}-aim.png"), keys=aiming, yaw=40)
            preview(model, os.path.join(args.preview, f"{file}-aim-side.png"), keys=aiming, yaw=95, pitch=5)


if __name__ == "__main__":
    main()
