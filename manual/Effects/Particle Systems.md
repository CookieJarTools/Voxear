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

Turn the Obj to aim a Cone or a Box. Particles move with their Obj, so a torch's flame stays on the torch as it's carried.

## How each particle lives

Settings with two numbers are a range: each particle picks somewhere between them, so no two are quite alike.

- **Lifetime**: how many seconds it lives.
- **Speed**: how fast it sets out, in metres a second.
- **Size**, and **End Size**: how wide it is as it starts, in metres, and how much that grows or shrinks by the time it dies.
- **Colour** and **Opacity**, and **End Colour** and **End Opacity**: it fades from one to the other as it ages. An End Opacity of 0 fades it out.
- **Gravity**: 1 falls as anything does, 0 floats, and below 0 rises, as smoke and steam do.
- **Drag**: how quickly the air slows it. Sparks need little; smoke and dust need more.
- **Spin**: how fast it turns, in degrees a second.

## How particles look

- **Blend**: **Alpha** particles cover what's behind them, as smoke and dust do. **Additive** particles add their light to it, as sparks, fire and magic do, and never darken anything.
- **Intensity**: how much brighter than their colour they glow. Sparks and fire want 2 or more, so they glow and bloom.
- **Lit**: lit by the sun, the sky and lamps, as smoke is. Turn it off for particles that make their own light, such as sparks.
- **Softness**: how far before the ground or a wall they fade out, in metres, rather than cutting a hard line where they meet it.
- **Seed**: two systems that are alike but for their Seed send out different particles.

## From scripts

`Obj.Particles` is the Obj's Particle System, and `Obj.AddParticles()` gives an Obj one. Every setting above can be read and changed. `Play()` starts it over from the start of a round, with its burst, and `Stop()` sends out no more particles while those already out live on.

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
```

A behaviour can also have a `ParticleSystem` field, picked in the Inspector, to reach another Obj's system.

## What's coming

Particles today are worked out on the graphics card from their age alone, so a system costs the same however long it runs. Still to come: particles that bounce off the world, trails, flipbook textures, a gallery of ready-made effects, and editing an effect as a stack of modules or a graph.
