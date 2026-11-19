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

Every Obj with a **Nav Agent**, a **Block Mob**, a **Mob Motion**, a **Motion Matcher** or an **Animator** is in a tier. Nothing needs adding.

## What each tier does less of

- **Planning:** between its plans, a Nav Agent goes on the way it last planned, at the velocity it last chose. It plans at once whenever it is sent somewhere new or a search it asked for is waiting.
- **Moving:** a far Nav Agent moves every few steps, each move covering the time of the steps it missed.
- **Posing:** a Mob Motion or Motion Matcher keeps its pose between poses, then poses for the whole time since. A Motion Matcher whose clips move its Obj (root motion), or that is playing an action, is posed every frame wherever it is.
- **Motion matching:** beyond the near tier a Motion Matcher searches less often (its **Search Every** times **Search time ×**), and lets its feet follow its clips rather than holding them where they came down.

Box models are cheap to pose and draw however many there are: each box follows its bone where it is drawn, with no skinning. Mob Motion poses them on several threads.

## Skinned characters

Characters made of skinned meshes, such as imported humans, are skinned on the GPU into **posed vertices**, which every pass then draws. What that costs is kept in bounds:

- **Room of their own.** Posed vertices have a pool set in **Project Settings ▸ Quality ▸ Characters ▸ Posed vertices**: a million by default, about forty characters of 8,000 vertices. A character there is no room for is posed where it is drawn instead, by its bones, which costs a little more in each pass but is never left standing at rest. At 0 every character is posed that way.
- **Only what is seen.** A character no camera can see, nor see its shadow, keeps the pose it was last skinned in until one can.
- **Shared poses.** Beyond the near tier, an **Animator** whose clip loops, with no root motion, plays it at the nearest fifteenth of a second, and every such character on the same clip of the same model shares that one pose: a hundred walkers far off are skinned a handful of times.
- **Played on the GPU.** In the far tier such an Animator is not posed at all. Its clip is baked the first time it plays on a model, and the GPU poses the character from the baked frames: five thousand walkers far off cost about what a thousand posed ones did. Anything hanging from the character, such as a held sword, keeps it posed as usual, so the sword follows its hand.

## Setting the tiers

Open **Project Settings ▸ Crowds**:

- **Tiers** turns tiers on or off and sets the **Near** and **Middle** distances.
- **Middle** and **Far** set how many steps between plans and moves, how many frames between poses, and how much longer a Motion Matcher waits between searches.

## One character's own tier

Choose **Add Component ▸ Navigation ▸ Crowd Member** on an Obj to put its tier in its own hands:

- **Tier:** **By Distance** as the rest, or always **Near**, **Middle** or **Far**. A boss that must never stutter is always Near; a flock of birds overhead can always be Far.
- **Importance:** scales the distances it is measured by. At 2 it stays near twice as far out; at 0.5, half as far.

## Seeing it work

The Profiler's counts show **Posed parts**: how many skinned and how many posed by their bones; **Posed elsewise**: how many posed where drawn for want of room, left as they were out of sight, sharing another's pose, and playing a baked clip; and **Posed at rest**, which should be none.
