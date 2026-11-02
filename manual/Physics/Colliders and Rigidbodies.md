---
order: 1
---

Physics in Voxear works the way it does in Unity. Three components decide what an Obj does:

- A **Collider** gives an Obj a shape that other things bump into. On its own, the Obj stays where you put it: a floor, a wall, a rock.
- A **Rigidbody** beside a Collider hands the Obj over to physics. It falls, tumbles, slides and gets knocked about.
- A **Character Controller** is for the player and other characters. It walks where your code tells it, climbs steps and slopes, and slides along walls rather than bouncing off them.

Add each one with **Add Component**, under **Physics**. Physics only runs while the game plays, so in the editor everything stays where you left it.

## Colliders

| Setting | What it does |
| --- | --- |
| Shape | Box, Sphere or Capsule |
| Center | Where the middle of the shape is, from the Obj's origin |
| Size | A box's width, height and depth |
| Radius | A sphere's or a capsule's radius |
| Height | A capsule's height, from end to end, standing up |
| Is trigger | Nothing bumps into it. Instead, it notices what passes through it (see below) |
| Friction | How much it grips what slides on it. 0 is ice, and the 0.6 it starts with suits most things |
| Bounciness | How much things bounce off it. 0 is a sack of sand, and 1 bounces back as high as it fell from |

The Inspector shows only the settings the chosen shape uses. When you select an Obj, the Scene view outlines its collider in green.

A collider grows and shrinks with its Obj's **Scale**. A box stretches along each axis. A sphere or a capsule can't stretch one way only, so it takes the largest of the scales across it.

Pick the simplest shape that fits. A box for a crate, a sphere for a ball, a capsule for anything tall and round, like a barrel or a person. Physics works these out far faster than anything shaped like the model itself, and players rarely notice the difference.

## Rigidbodies

| Setting | What it does |
| --- | --- |
| Mass | How heavy it is, in kilograms. A heavy body pushes a light one aside |
| Is kinematic | Physics stops moving it. Your code or an animation moves it instead, and it pushes whatever is in its way. Use it for lifts, moving platforms and doors |
| Use gravity | Whether it falls. Turn it off for things that float, like a balloon or a ship in space |
| Drag | How quickly it slows down as it moves through the air |
| Angular drag | How quickly it stops spinning |
| Continuous | Checks the whole path the body took in each step, so something fast, like a bullet, can't pass through a thin wall between one step and the next. It costs more, so only turn it on for fast, small things |

A body that has come to rest goes to sleep and costs nothing until something hits it, or your code moves it. It still counts as touching what it rests on.

Physics moves bodies in fixed steps, 50 times a second, straight after each `FixedUpdate`, so physics code belongs in `FixedUpdate` as it does in Unity. Between steps, the Obj's Transform shows where physics last put it.

## Character Controllers

| Setting | What it does |
| --- | --- |
| Height | How tall the character is, from its feet up |
| Radius | How wide it is |
| Slope limit | The steepest slope it can walk up, in degrees. Anything steeper, it slides down |
| Step offset | The highest step it climbs without jumping, in metres |

The character's capsule stands on the Obj's origin, so the Obj's position is where its feet are. Physics never pushes a character around on its own. It moves only when your code moves it, and gravity is up to your code too, so a jump is just a velocity upwards.

## Triggers

A collider with **Is trigger** turned on is a zone rather than a solid thing. Nothing bumps into it, but physics notices when another collider enters it and when it leaves. Use triggers for checkpoints, doors that open as the player comes near, and pickups.

A trigger notices only colliders that physics moves: Objs with a Rigidbody, and characters. Two still colliders never touch each other.

## In scene files

Each component is a table on its Obj, and leaves out the settings you didn't change:

```toml
[[obj]]
id = 2
name = "Crate"
position = [0.0, 5.0, 0.0]
collider = {}
rigidbody = { mass = 20.0 }

[[obj]]
id = 3
name = "Checkpoint"
collider = { size = [4.0, 3.0, 1.0], trigger = true }

[[obj]]
id = 4
name = "Player"
character = { height = 1.7, radius = 0.35 }
```

Prefabs keep their physics too. An instance that changes its prefab's collider saves the whole collider with the instance, as it does for lights and cameras.
