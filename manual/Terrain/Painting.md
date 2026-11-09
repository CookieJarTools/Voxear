---
order: 8
---

Pick **Paint** along the top of the Terrain in the Inspector, pick a surface in its list (see [[Surfaces]]), and paint it on the ground in the Scene view with the left mouse button. A ring on the ground shows the brush, with an inner ring where it is strongest.

Painting lays the picked surface over the others: it covers more where the brush goes, and the surfaces above it in the list cover less there, so it shows. Hold **Shift** to take the picked surface away instead, showing what is below it. The ground's own surface, at the bottom of the list, is painted by taking the others away.

Each stroke is one step to undo.

| Setting | What it does |
| --- | --- |
| Size | The brush's radius, in metres |
| Strength | How fast it paints, 0 to 1 |

**[** and **]** make the brush smaller and larger, and with **Shift** weaker and stronger. The brush is the one Sculpt uses, so its size carries from one tool to the other.

## Painting and rules

What you paint adds to a surface's cover from its rules, or takes from it. Snow kept above 80 metres by its rules can be painted lower down a gully, or taken off a path along a ridge, and the rest of its edge still follows the ground. Changing the rules keeps what was painted.

## What painting keeps

Each surface's painting is kept in the terrain's `.layers` file, beside what the sculpting brushes painted. The `.heights` file keeps each point's surfaces as they are drawn, so a built game, which has no `.layers` file, reads them without working them out.
