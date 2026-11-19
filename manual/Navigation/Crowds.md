---
order: 13
---

A world can have hundreds of characters walking about, and most of them are far from the camera. **Crowds** gives each one a **tier** every frame by how near it is, and does less for those further out, so a town full of villagers costs little more than the few beside you.

| Tier | Where | What it gets |
| --- | --- | --- |
| Near | Within 40 m | Everything, every step and frame |
| Middle | Out to 150 m | Plans its way every 2nd step, posed every 2nd frame |
| Far | Beyond that | Plans every 4th step, moves every 2nd, posed every 4th frame |

Distances are measured from the game's camera, and on a server from every player too, so a character near any player is near. With no camera at all, as on a server with no players yet, every character is near.

Every Obj with a **Nav Agent**, a **Block Mob**, a **Mob Motion** or a **Motion Matcher** is in a tier. Nothing needs adding.

## What each tier does less of

- **Planning:** between its plans, a Nav Agent goes on the way it last planned, at the velocity it last chose. It plans at once whenever it is sent somewhere new or a search it asked for is waiting.
- **Moving:** a far Nav Agent moves every few steps, each move covering the time of the steps it missed.
- **Posing:** a Mob Motion or Motion Matcher keeps its pose between poses, then poses for the whole time since. A Motion Matcher whose clips move its Obj (root motion), or that is playing an action, is posed every frame wherever it is.
- **Motion matching:** beyond the near tier a Motion Matcher searches less often (its **Search Every** times **Search time ×**), and lets its feet follow its clips rather than holding them where they came down.

Box models are cheap to pose and draw however many there are: each box follows its bone where it is drawn, with no skinning. Mob Motion poses them on several threads.

## Setting the tiers

Open **Project Settings ▸ Crowds**:

- **Tiers** turns tiers on or off and sets the **Near** and **Middle** distances.
- **Middle** and **Far** set how many steps between plans and moves, how many frames between poses, and how much longer a Motion Matcher waits between searches.

## One character's own tier

Choose **Add Component ▸ Navigation ▸ Crowd Member** on an Obj to put its tier in its own hands:

- **Tier:** **By Distance** as the rest, or always **Near**, **Middle** or **Far**. A boss that must never stutter is always Near; a flock of birds overhead can always be Far.
- **Importance:** scales the distances it is measured by. At 2 it stays near twice as far out; at 0.5, half as far.

## Seeing it work

The Profiler's counts show **Posed parts**: how many skinned, how many posed by their bones, and how many were left at rest for want of room, which should be none.
