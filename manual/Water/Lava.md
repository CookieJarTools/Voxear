---
order: 4
---

Set a [[Pools and Dry Areas|Pool's]] or a [[Streams|Stream's]] **Liquid** to **Lava**, and it fills with molten rock instead. Lava is thick and slow. It glows by its temperature, as hot rock does: yellow-white where it wells up, orange under its skin, dull red as it cools, then black. It has no waves, and nothing sees through it.

## How lava cools

Fresh lava is molten all over. Within seconds a skin sets on it, in cells a hand's width across, with bright seams between them where the rock underneath still shows.

As it goes on cooling, crust forms: dark plates of rock, each growing out from its middle, so islands of crust stand in molten rock before they close up. Some stretches set sooner than others. The crust stands above the molten rock in the gaps. Look along the lava and the near plate's edge hides a gap; look down into one and you see the glowing rock at its bottom, and the far plate's side lit by it.

As it cools further the gaps narrow to cracks, and the crust thickens and dulls.

**Cooling** sets how many seconds all that takes. **Glow** is how bright the lava shines.

## Lava flows

A Stream of lava stands on the ground as a thick body: as high as its **Depth** in the middle, its sides rounding down to the ground, and each end rounded off into a lobe. It runs a dozen times slower than water down the same slope.

Each part of a flow knows how long ago it set out, and cools as it goes:

- **At its source**, it is molten and bright, drawn out into streaks along the flow.
- **Its banks** crust over first, into levees that close in on a channel of molten rock down its middle.
- **The channel** runs on molten long after the banks have set, for about twice its Cooling.
- **Its plates of crust** ride along with the flow and crowd together where it slows.
- **While the crust is soft**, it folds into ropes across the flow, bowed downstream where the middle runs faster than the edges: pahoehoe, as on Hawaii's lava fields.
- **Far from its source**, it has set into dark rock, with only a faint glow in its deepest cracks.

Lava that runs into the sea crusts over at once.

## Lava lakes

A Pool of lava is a lava lake. Its surface turns over in patches, every quarter of a minute or so: molten rock wells up in one place while plates of crust drift and jostle in another, and the patches wander across the lake.

**Cooling** sets how much of a lake has crusted: a few seconds for one that is nearly solid, a minute for one that is mostly molten.

Raised above the ground, a lake's sides are rock with glowing seams, hotter deeper down.

## Bubbles and spatter

Gas comes up through lava wherever it is still molten. A bubble swells slowly into a dome, up to a metre across, breaking open any crust over it. Its skin thins and brightens as it stretches, and then it bursts. The burst throws up blobs of molten rock that arc up, cool from yellow to red as they fly and fall back, and it leaves a ring of fresh rock that spreads and heals.

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
