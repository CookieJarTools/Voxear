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
| Sparks | A stream of sparks with tails, that bounce where they land |
| Dust | Dust hanging over a patch of ground |
| Rain, Snow | Falling from a box 14 metres across at the Obj, so put it overhead |
| Blood | A spurt that lands and stays a moment |
| Muzzle Flash | A flash, sparks and a wisp of smoke, once |
| Explosion | Flash, fireball, smoke, sparks, tumbling debris and a shockwave, once |
| Magic | Glowing motes that draw threads of light, round a pulsing light |
| Mud | Clods and spatter thrown up, once |

In a game an effect that plays once plays once each time it starts: `Play()` starts it over.

### Making your own effect

Choose **New Particle Effect…** in the Project panel and pick the starter to begin from. That makes a `.particles` file, a copy that's yours to change, and it shows in every Particle System's Effect list.

Double-click the file to open the effect on its own, in place of the level, as a prefab opens:

- **Each emitter is an Obj** with a Particle System. Select one and the Inspector shows its settings, everything on this page.
- **Add an emitter** with the Hierarchy's **+ ▸ Particle System**, take one out by deleting its Obj, and rename an Obj to rename its emitter. Emitters draw in the Hierarchy's order, the last on top.
- **Save** (Ctrl+S) writes the effect, and every Particle System playing it takes up the change.
- **Back**, on the bar over the Scene view, returns to the level.

Only the Particle Systems are the effect: where an emitter's Obj stands isn't kept, nor anything else you put on it.

## Watching particles play

While a Particle System is selected, or an effect is open, a strip at the foot of the Scene view plays it:

- **Pause** and **Play** hold the particles still and let them go.
- **Restart** plays from the start.
- The **speed** button goes round 1×, 0.5×, 0.25×, 0.1× and 2×, for watching a fast effect slowly.
- Drag the **scrubber** to hold the effect at a time. Particles the wind or a collision moved are shown where their age alone has them while you scrub.

An effect that plays once, as an explosion does, plays over again when it ends, until you pause it. Only the editor's view of the level is paused or slowed: a game plays particles at full speed.

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

## Trails

**Trails** is what a system's particles leave behind them.

| Trails | What is drawn |
| --- | --- |
| Off | Only the particles |
| Particles | Each particle with a tail along the way it has come, narrowing and fading to its end |
| Ribbon | One ribbon through all the particles, in the order they were sent out, and not the particles themselves |

A tail is for sparks, embers and shooting stars. Unlike **Stretch** it follows the path the particle really took, so it bends round a bounce and curls in turbulence. **Trail Time** is how long it is, in seconds of the particle's way: a spark going 6 metres a second with a Trail Time of 0.1 has a tail 60 centimetres long.

A ribbon is for the streak behind a spell, a blade or a rocket. Put the system on the Obj that moves, set its **Space** to **World** so its particles stay behind, its **Shape** to **Point** so they start in a line, and its **Speed** to 0. The ribbon then runs from the Obj back through every particle still alive, so its **Lifetime** is how long the streak lasts, and its colour and size over life are how it fades and narrows along its length. Raise the **Rate** if a fast turn shows corners: 30 to 60 a second suits most.

**Trail Width** is how wide a tail or the ribbon is, against the particles: 1 as wide as they are.

Trails are coloured and lit as their particles are, and drawn soft-edged without the Texture. A particle with a tail takes the room of six in the [budget](#the-particle-budget). A ribbon takes no more than its particles.

## Simulated systems

A system using none of these settings works out where each particle is from its age alone. Nothing is kept from frame to frame, and it costs almost nothing.

Once a system uses any of them, its particles are **simulated**: kept on the graphics card and moved a step each frame. The **Simulated** row at the bottom of the component says which settings have made it so, or No. You don't choose between the two, and a simulated system looks the same as one that isn't until something moves it.

Simulated systems are still cheap. A million particles are stepped in about a fifth of a millisecond on a mid-range graphics card. A system whose last particle has died costs nothing until it plays again.

## The particle budget

Every simulated system has room for as many particles as it can have out at once: its Rate times its longest Lifetime, plus its Burst. Particles with tails take six times as much. **Project Settings ▸ Quality ▸ Budget** is the most room all of them may have between them, a million particles to begin with. A system there's no room for isn't drawn, and the Console says so once. In `voxear.toml` the budget is `particles` under `[quality]`. Each particle takes 36 bytes of graphics memory, so the full million is 36 MB, taken only as systems need it.

The stats overlay (**F1** in a game) and the Profiler show **Particles**: how many are alive, of the budget. The Profiler's GPU list has **Particle simulation** and **Particles**, the time spent stepping and drawing them.

## How particles look

- **Blend**: **Alpha** particles cover what's behind them, as smoke and dust do. **Additive** particles add their light to it, as sparks, fire and magic do, and never darken anything.
- **Intensity**: how much brighter than their colour they glow. Sparks and fire want 2 or more, so they glow and bloom.
- **Lit**: lit by the sun, the sky and lamps, as smoke is. Turn it off for particles that make their own light, such as sparks. A lit **Puff** is lit from six sides: each puff is brighter toward the sun or a lamp and darker away from it, so smoke has shape, and lets light from behind through its thin edges.
- **Softness**: how far before the ground or a wall they fade out, in metres, rather than cutting a hard line where they meet it.
- **Texture**: what each particle is drawn with. **Dot** is a soft round dot. **Puff**, **Flame**, **Ring**, **Flare** and **Shard** are built in, most of them four different shapes so no two particles need look alike, and white, so they take the particle's colour. Or pick one of the project's pictures.
- **Tiles**: for a picture of your own that holds several, as a flipbook does: how many across and down. Each particle is drawn with one of them.
- **Animate**: each particle plays through its texture's pictures over its life, rather than keeping one. Each picture fades into the next, so a few play smoothly.
- **Light Map**: for a lit picture of your own, a second picture that lights it from six sides, as smoke baked in a simulation tool comes: the Texture says how bright each pixel is lit from the right (red), from above (green) and from behind (blue), with its shape in alpha, and the Light Map from the left, from below and from in front.
- **Light**: how bright a light the system casts on what's round it, as a Point Light's Intensity, in the colour its particles start with. A stream's flickers, as a fire does. A burst's flashes and fades as its particles die, as an explosion's does. **Light Range** is how far it reaches, in metres. The 16 nearest the camera shine.
- **Stretch**: draws each particle longer along the way it's going, in metres for each metre a second: sparks and rain become streaks. A stretched particle doesn't spin.
- **Mesh**: draws each particle as a mesh in place of a flat picture. See [Mesh particles](#mesh-particles).
- Particles sit in the scene's fog and haze as far off as they are: near ones show clearly, far ones fade into the mist.
- **Seed**: two systems that are alike but for their Seed send out different particles.

## Mesh particles

**Mesh** draws each particle as a solid shape that tumbles through the air: rubble from an explosion, clods of earth, shell cases, splinters. Pick a built-in one or one of the project's models.

| Mesh | What it is |
| --- | --- |
| Cube | A box |
| Ball | A ball |
| Chunk | A lump with flat faces, no two sides alike |
| Rod | A short rod, three and a half times as long as it is wide |

- **Size** is how wide each one is across its furthest corners, in metres, whatever size the model was made.
- **Spin** is how fast each tumbles, in degrees a second, round an axis of its own. At 0 each keeps the turn it started with.
- **Color** tints the mesh's own material. A built-in mesh is plain white, so it takes the colour as it is. A model keeps its own colours, roughness and metal under the tint.
- **Lit** particles are lit as any surface is, by the sun, the sky and lamps. Unlit ones show their colour as it is, for things that glow.
- As a mesh particle fades it thins out pixel by pixel rather than turning clear, so fade it only over the last tenth of its life or so.

A model's first mesh is used, at its full detail, so keep models for particles simple: a few dozen triangles each is plenty for debris. Mesh particles hide one another and the flat particles behind them, as solid things do. They don't cast shadows.

Give them **Collision** with a little **Bounce** and plenty of **Friction** and they land and come to rest.

## From scripts

`Obj.Particles` is the Obj's Particle System, and `Obj.AddParticles()` gives an Obj one. Every setting above can be read and changed but the Effect, Texture and Mesh, which are picked in the Inspector. `Play()` starts it over from the start of a round, with its burst, and `Stop()` sends out no more particles while those already out live on. `IsSimulated` says whether its settings have it simulated. `Particles.Push` sets off a blast that throws the particles of every system that takes forces (see [[Force Fields]]).

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

Still to come: collision with what's out of view, a picture along a ribbon, mesh particles that cast shadows, mist and dust in the fog, and editing an effect as a graph.
