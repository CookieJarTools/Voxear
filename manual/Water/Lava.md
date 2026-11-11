---
order: 4
---

Set a [[Pools and Dry Areas|Pool's]] or a [[Streams|Stream's]] **Liquid** to **Lava**, and it fills with molten rock instead. Lava is thick and slow. It glows by its temperature, as hot rock does: yellow-white only where it has just welled up or burst, orange across a stretch of molten rock, deep red against the crust, then black. It has no waves, and nothing sees through it.

## How lava cools

Fresh lava is molten all over, and it heaves: slow swells a couple of metres across rise and sink over several seconds. Within seconds a skin sets on it, in cells a hand's width across. The skin is orange, redder the thicker it has set, and the rock underneath shows yellow in the seams where it has torn. Where the skin is pushed against crust it gathers into folds along the crust's edge.

As it goes on cooling, crust forms: dark plates of rock, each growing out from its middle, so islands of crust stand in molten rock before they close up. Some stretches set sooner than others. No two plates have the same shape, their edges wander, and they meet three at a corner, as cracks do.

Crust is thick, and it stands above the molten rock in the gaps. Look along the lava and the near plate's edge hides a gap; look down into one and you see the glowing rock at its bottom, deep red where it meets the plates, and the far plate's side lit by it. While a plate was soft it was folded into ropes round its rim, where it was pushed against the next one, and the rim stands a little proud. Each plate leans a little its own way.

As it cools further the gaps narrow, and the crust thickens and dulls. In some stretches they stay open as leads of molten rock a stride across; in most they close to cracks. Crust does not set evenly. In places, and most along its edges, it breaks up into smaller pieces that stand a little higher or lower than each other, with the rock under them glowing dull red in the cracks between.

**Cooling** sets how many seconds all that takes. **Glow** is how bright the lava shines.

## Lava flows

A Stream of lava stands on the ground as a thick body: as high as its **Depth** in the middle, its sides rounding down to the ground, and each end rounded off into a lobe. It runs a dozen times slower than water down the same slope.

Each part of a flow knows how long ago it set out, and cools as it goes:

- **At its source**, it is molten and bright, drawn out into streaks along the flow.
- **Its banks** crust over first, into levees that close in on a channel of molten rock down its middle.
- **The channel** runs on molten long after the banks have set, for about twice its Cooling.
- **Its plates of crust** ride along with the flow and crowd together where it slows.
- **While the crust is soft**, it folds into ropes across the flow, bowed downstream where the middle runs faster than the edges, and is drawn out into strings along it: pahoehoe, as on Hawaii's lava fields.
- **Far from its source**, it has set into dark rock, with only a faint glow in its deepest cracks.

Lava that runs into the sea crusts over at once.

## Lava lakes

A Pool of lava is a lava lake. Its surface turns over in patches a few metres across, every quarter of a minute or so: molten rock wells up in one place while plates of crust drift and jostle in another, and the patches wander across the lake. A lake more than a few metres wide has both to show at any moment.

**Cooling** sets how much of a lake has crusted: a few seconds for one that is nearly solid, a minute for one that is mostly molten.

Raised above the ground, a lake's sides are rock with glowing seams, hotter deeper down.

## Bubbles and spatter

Gas comes up through lava wherever it is still molten. A bubble swells slowly into a dome, up to a metre across, breaking open any crust over it. Its skin thins and brightens as it stretches, and then it bursts.

A burst is a small explosion. Most of what it throws is a spray of fine drops, fast and wide, drawn out into streaks as they fly. With them go heavier drops, and a few clots a hand across that barely clear the lava. A large bubble throws well over a hundred of them, as high as two or three metres; a small one throws fewer, lower. They cool from yellow-white to red as they fly, the smallest soonest, and fall back. The lava spits a little more for a moment after, and the burst leaves a ring of fresh rock that spreads and heals.

Here and there a vent spits small bursts one after another from the same place.

How much of this there is follows the lava. A molten lake has a few bubbles going at any moment. A flow bubbles down its channel, most near its source. Lava that has crusted over lies still.

Every machine sees the same bubbles at the same moments, so players in one game see the same lava.

## The light lava gives

Lava lights what stands round it. Each lake, and each stretch of a flow that still glows, is a light as wide as it is, as bright as how much of it is molten. The light follows **Glow**.

These lights cast no shadows, so their light reaches a few metres through a wall. Where that shows, add a [[Lights|Light]] of your own that casts shadows and turn the lava's Glow down. A frame has at most sixteen of them, the ones nearest the camera.

## Splashes in lava

An Obj with a Rigidbody that falls into lava, or is pushed through it, throws up blobs of molten rock where it goes in. For anything that moves without a Rigidbody, ask for a splash from a script:

```csharp
Water.Splash(Transform.Position, 0.5f);
```

[[Floating Objects]] has more about splashes.

## Lava's settings

| Setting | What it does |
| --- | --- |
| Cooling | How many seconds it takes to crust over |
| Glow | How brightly it glows, and how much it lights what is round it |
| Depth | On a Stream, how high the flow stands above its bed, in metres |
| Speed | On a Stream, how fast it runs where its bed is level, in metres a second |

## Lava on a mesh of your own

The built-in **Lava** shader (see [[Custom Shaders]]) draws lava on any mesh, for a pool of any shape or a lava fall. It is the same glowing rock under drifting plates, without the relief, the bubbles or the light.
