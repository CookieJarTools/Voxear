---
order: 1
---

A **Particle System** sends out particles from its Obj: small soft dots that fly, fall, grow and fade. Many together make sparks, smoke, spray, dust or magic. Make one with the Hierarchy's **+ ▸ Particle System**, or add one to any Obj with **Add Component ▸ Effects ▸ Particle System**. It starts playing straight away, in the Scene view too, so you see each change as you make it.

## How particles are sent out

Each round lasts its **Duration**. While it runs, the system sends out **Rate** particles a second, and a **Burst** of them all at once as it starts. With **Looping** on it starts over when the round ends; with it off, it sends out no more after one round, and the last particles live out their lives.

Each particle sets out from the system's **Shape**:

| Shape | Where from, and which way |
| --- | --- |
| Point | The Obj's middle, every way |
| Sphere | Inside a ball of its **Radius**, outwards |
| Cone | A disc of its **Radius**, up the Obj's green arrow (Y), spreading out to its **Angle** |
| Box | Inside a box of its **Box Size**, up the Obj's Y |

Turn the Obj to aim a Cone or a Box. Particles move with their Obj, so a torch's flame stays on the torch as it's carried. Set **Space** to World to leave them behind instead (see below).

## How each particle lives

Settings with two numbers are a range: each particle picks somewhere between them, so no two are quite alike.

- **Lifetime**: how many seconds it lives.
- **Speed**: how fast it sets out, in metres a second.
- **Size**, and **End Size**: how wide it is as it starts, in metres, and how much that grows or shrinks by the time it dies.
- **Colour** and **Opacity**, and **End Colour** and **End Opacity**: it fades from one to the other as it ages. An End Opacity of 0 fades it out.
- **Gravity**: 1 falls as anything does, 0 floats, and below 0 rises, as smoke and steam do.
- **Drag**: how quickly the air slows it. Sparks need little; smoke and dust need more.
- **Spin**: how fast it turns, in degrees a second.

## Changing over a particle's life

Four settings change a particle as it ages, from when it sets out (the left of the graph) to when it dies (the right):

- **Size Over Life**: how wide it is, times its Size.
- **Opacity Over Life**: how opaque it is, times its Opacity.
- **Speed Over Life**: how fast it flies, times the speed it set out at. Start high and fall to 0 for a puff that bursts out and hangs.
- **Colour Over Life**: its colour, from one colour to the next.

Each is empty to start with, and does nothing until you give it keys. Then it takes the place of the End Size, End Opacity or End Colour.

- **Curve:** press on the graph to add a key there. Drag a key to move it, and drag it off the graph and let go to take it out.
- **Gradient:** press on the strip to add a colour, drag its marker to move it, and type its red, green and blue under the strip.

## Wind, turbulence and space

Three settings let the air and the world take hold of particles once they're out:

- **Space**: **Local** particles move and turn with their Obj. **World** particles stay where they are once sent out, so a moving Obj leaves a trail of them: the smoke behind a torch, the dust behind a wheel.
- **Wind**: how much of the level's wind they pick up. At 0 they ignore it, and at 1 they're carried along at its speed, as smoke is. Sparks and debris want a little, smoke and dust a lot. The wind is the level's Wind Zone (see [[Wind]]): the same gusts that bend the grass take the smoke over it.
- **Turbulence**: how fast swirls in the air stir them about, in metres a second. A little makes smoke curl and embers wander. **Turbulence Size** is how wide those swirls are, in metres: small for a candle, large for a bonfire.

### Simulated systems

A system using none of these three works out where each particle is from its age alone. Nothing is kept from frame to frame, and it costs almost nothing.

Once a system uses any of them, its particles are **simulated**: kept on the graphics card and moved a step each frame. The **Simulated** row at the bottom of the component says which settings have made it so, or No. You don't choose between the two, and a simulated system looks the same as one that isn't until the wind or turbulence moves it.

Simulated systems are still cheap. A million particles are stepped in about a fifth of a millisecond on a mid-range graphics card. A system whose last particle has died costs nothing until it plays again.

### The particle budget

Every simulated system has room for as many particles as it can have out at once: its Rate times its longest Lifetime, plus its Burst. **Project Settings ▸ Quality ▸ Budget** is the most room all of them may have between them, a million particles to begin with. A system there's no room for isn't drawn, and the Console says so once. In `voxear.toml` the budget is `particles` under `[quality]`. Each particle takes 36 bytes of graphics memory, so the full million is 36 MB, taken only as systems need it.

The stats overlay (**F1** in a game) and the Profiler show **Particles**: how many are alive, of the budget. The Profiler's GPU list has **Particle simulation** and **Particles**, the time spent stepping and drawing them.

## How particles look

- **Blend**: **Alpha** particles cover what's behind them, as smoke and dust do. **Additive** particles add their light to it, as sparks, fire and magic do, and never darken anything.
- **Intensity**: how much brighter than their colour they glow. Sparks and fire want 2 or more, so they glow and bloom.
- **Lit**: lit by the sun, the sky and lamps, as smoke is. Turn it off for particles that make their own light, such as sparks.
- **Softness**: how far before the ground or a wall they fade out, in metres, rather than cutting a hard line where they meet it.
- Particles sit in the scene's fog and haze as far off as they are: near ones show clearly, far ones fade into the mist.
- **Seed**: two systems that are alike but for their Seed send out different particles.

## From scripts

`Obj.Particles` is the Obj's Particle System, and `Obj.AddParticles()` gives an Obj one. Every setting above can be read and changed. `Play()` starts it over from the start of a round, with its burst, and `Stop()` sends out no more particles while those already out live on. `IsSimulated` says whether its settings have it simulated.

```csharp
// A burst of sparks where the hammer lands.
var sparks = hit.Obj.AddParticles();
sparks.Looping = false;
sparks.Rate = 0f;
sparks.Burst = 40;
sparks.Speed = new Vector2(2f, 5f);
sparks.Gravity = 1f;
sparks.Blend = ParticleBlend.Additive;
sparks.Lit = false;
sparks.Intensity = 3f;
sparks.Play();

// Smoke that drifts off downwind, curling, and stays behind as its Obj moves.
var smoke = chimney.AddParticles();
smoke.Space = ParticleSpace.World;
smoke.Wind = 1f;
smoke.Turbulence = 0.6f;
smoke.TurbulenceSize = 2f;
```

A behaviour can also have a `ParticleSystem` field, picked in the Inspector, to reach another Obj's system.

## What's coming

Still to come: particles that bounce off the world and are pushed by blasts, trails, textures and flipbooks, smoke lit from every side, a gallery of ready-made effects, and editing an effect as a stack of modules or a graph.
