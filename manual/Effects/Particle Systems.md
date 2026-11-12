---
order: 1
---

A **Particle System** sends out particles from its Obj: small soft dots that fly, fall, grow and fade. Many together make sparks, smoke, spray, dust or magic. Make one with the Hierarchy's **+ ▸ Particle System**, or add one to any Obj with **Add Component ▸ Effects ▸ Particle System**. It starts playing straight away, in the Scene view too, so you see each change as you make it.

## Effects

The quickest way to something good is the component's **Effect**: pick one and the Particle System plays it in place of its own settings. An effect is several emitters played together, so an explosion is its flash, its fireball, its smoke, its sparks and the debris it throws, all from one Obj.

Every project has these starters:

| Starter | What it is |
| --- | --- |
| Fire | Flames, the smoke they leave downwind, and embers |
| Smoke | A column of smoke that drifts with the wind |
| Sparks | A stream of sparks that bounce where they land |
| Dust | Dust hanging over a patch of ground |
| Rain, Snow | Falling from a box 14 metres across at the Obj, so put it overhead |
| Blood | A spurt that lands and stays a moment |
| Muzzle Flash | A flash, sparks and a wisp of smoke, once |
| Explosion | Flash, fireball, smoke, sparks, debris and a shockwave, once |
| Magic | Glowing motes round a pulsing light |
| Mud | Clods and spatter thrown up, once |

An effect that plays once plays over again while its Obj is selected in the editor, so you can watch it. In a game it plays once each time it starts: `Play()` starts it over.

To make an effect of your own, choose **New Particle Effect…** in the Project panel and pick the starter to begin from. That makes a `.particles` file, a copy that's yours to change, and it shows in every Particle System's Effect list. Selecting it shows its emitters. The file is text: each `[[emitter]]` has a `name` and then the same settings a Particle System has, written as a scene writes them, and a Particle System playing it takes up your changes as you save. Editing an effect's emitters in the editor itself is still to come.

With Effect set to None, the Particle System uses its own settings, the rest of this page.

## How particles are sent out

Each round lasts its **Duration**, and starts its **Delay** after the system does. While it runs, the system sends out **Rate** particles a second, and a **Burst** of them all at once as it starts. With **Looping** on it starts over when the round ends; with it off, it sends out no more after one round, and the last particles live out their lives.

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

These settings let the air and the world take hold of particles once they're out:

- **Space**: **Local** particles move and turn with their Obj. **World** particles stay where they are once sent out, so a moving Obj leaves a trail of them: the smoke behind a torch, the dust behind a wheel.
- **Wind**: how much of the level's wind they pick up. At 0 they ignore it, and at 1 they're carried along at its speed, as smoke is. Sparks and debris want a little, smoke and dust a lot. The wind is the level's Wind Zone (see [[Wind]]): the same gusts that bend the grass take the smoke over it.
- **Turbulence**: how fast swirls in the air stir them about, in metres a second. A little makes smoke curl and embers wander. **Turbulence Size** is how wide those swirls are, in metres: small for a candle, large for a bonfire.

## Collision

Set **Collision** to **Depth** and particles land on what's in front of the camera, rather than passing through it: sparks bounce down steps, rain stops at the ground, debris piles against a wall.

- **Bounce**: how much of its speed into a surface a particle keeps, away from it. 0 stops dead, and 1 bounces as fast as it came.
- **Friction**: how much of its speed along the surface it loses each time it lands. 0 slides on, and 1 stops where it lands.
- **Life Lost**: how much of the life it has left it loses each time it strikes something. 1 and it dies there, as a raindrop does.

Depth collision reads what the main camera sees, which costs next to nothing. The price is that particles only meet what's in view: one that falls behind a wall, or off the edge of the screen, meets nothing there. That suits sparks, rain and debris in front of the player. Particles are for looks: nothing they hit feels it, so anything that must hurt or push is a physics body with a Particle System on it.

## Forces

**Forces** is how much [[Force Fields]], blasts and whoever passes through move a system's particles: 0 not at all, 1 in full. Dust and smoke want 1, so an explosion throws them and a runner parts them; heavy debris wants less.

## Simulated systems

A system using none of these settings works out where each particle is from its age alone. Nothing is kept from frame to frame, and it costs almost nothing.

Once a system uses any of them, its particles are **simulated**: kept on the graphics card and moved a step each frame. The **Simulated** row at the bottom of the component says which settings have made it so, or No. You don't choose between the two, and a simulated system looks the same as one that isn't until something moves it.

Simulated systems are still cheap. A million particles are stepped in about a fifth of a millisecond on a mid-range graphics card. A system whose last particle has died costs nothing until it plays again.

## The particle budget

Every simulated system has room for as many particles as it can have out at once: its Rate times its longest Lifetime, plus its Burst. **Project Settings ▸ Quality ▸ Budget** is the most room all of them may have between them, a million particles to begin with. A system there's no room for isn't drawn, and the Console says so once. In `voxear.toml` the budget is `particles` under `[quality]`. Each particle takes 36 bytes of graphics memory, so the full million is 36 MB, taken only as systems need it.

The stats overlay (**F1** in a game) and the Profiler show **Particles**: how many are alive, of the budget. The Profiler's GPU list has **Particle simulation** and **Particles**, the time spent stepping and drawing them.

## How particles look

- **Blend**: **Alpha** particles cover what's behind them, as smoke and dust do. **Additive** particles add their light to it, as sparks, fire and magic do, and never darken anything.
- **Intensity**: how much brighter than their colour they glow. Sparks and fire want 2 or more, so they glow and bloom.
- **Lit**: lit by the sun, the sky and lamps, as smoke is. Turn it off for particles that make their own light, such as sparks.
- **Softness**: how far before the ground or a wall they fade out, in metres, rather than cutting a hard line where they meet it.
- **Texture**: what each particle is drawn with. **Dot** is a soft round dot. **Puff**, **Flame**, **Ring**, **Flare** and **Shard** are built in, most of them four different shapes so no two particles need look alike, and white, so they take the particle's colour. Or pick one of the project's pictures.
- **Tiles**: for a picture of your own that holds several, as a flipbook does: how many across and down. Each particle is drawn with one of them.
- **Animate**: each particle plays through its texture's pictures over its life, rather than keeping one.
- **Stretch**: draws each particle longer along the way it's going, in metres for each metre a second: sparks and rain become streaks. A stretched particle doesn't spin.
- Particles sit in the scene's fog and haze as far off as they are: near ones show clearly, far ones fade into the mist.
- **Seed**: two systems that are alike but for their Seed send out different particles.

## From scripts

`Obj.Particles` is the Obj's Particle System, and `Obj.AddParticles()` gives an Obj one. Every setting above can be read and changed. `Play()` starts it over from the start of a round, with its burst, and `Stop()` sends out no more particles while those already out live on. `IsSimulated` says whether its settings have it simulated. `Particles.Push` sets off a blast that throws the particles of every system that takes forces (see [[Force Fields]]).

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

Still to come: editing an effect's emitters in the editor, collision with what's out of view, trails, flipbooks blended frame to frame, smoke lit from every side, and editing an effect as a graph.
