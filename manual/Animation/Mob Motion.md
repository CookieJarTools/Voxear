---
order: 9
---

**Mob Motion** animates a creature from how its Obj moves, with no clips to make. Move the Obj and it walks; stop and it stands and breathes. It is made for [[Box Models]], whose bones say what part of the body each one moves.

## Adding Mob Motion

Select an Obj that draws a box model and choose **Add Component ▸ Animation ▸ Mob Motion**. That is all it needs. Whatever moves the Obj, a script, a Character Controller or the editor's Move tool while the game plays, the model's legs step to match.

The model's bones need **roles**: Body, Head, Leg, Arm, Tail or Wing. **Make Bones** in the Model window sets them for you, and you can change one in the Inspector with a bone picked. A bone with no role is left alone.

## What Mob Motion does

- **Walking.** Legs swing by how far the Obj has travelled, not by time, so feet keep pace with the ground at any speed. Two legs step in turn. Four step as diagonal pairs. Six or more step as two sets.
- **Knees and elbows.** A leg or arm with a joint (see **Add Joint** in [[Box Models]]) bends there as it comes through.
- **Arms** swing against the legs.
- **The body** bobs with each step, leans into its speed, and rolls a little from foot to foot.
- **The head** turns to what it is told to look at, no further than a neck goes, and takes a moment to get there. With nothing to look at, it looks ahead and wanders slightly.
- **A tail** swings out when the creature turns and springs back. **Wings** lift a little with each step.
- **Standing still**, the body breathes and the arms sway.
- **An attack** swings the right arm up and over. A creature with no arms lunges with its whole body and dips its head.
- **A flinch**, for when it is hit, knocks the body back and shakes the head.

## Settings

| Setting | What it does |
| --- | --- |
| Leg Swing | How far the legs swing as it walks, in degrees |
| Arm Swing | How far the arms swing against the legs, in degrees |
| Stride | How many metres one step covers. Leave it at 0 to use what the legs' length gives. A longer stride steps more slowly at the same speed |
| Bounce | How much the body bobs and leans as it walks. 0 keeps it level |
| Idle | How much it breathes and shifts standing still. 0 stands like a statue |
| Look Speed | How fast the head turns to what it looks at |
| Look Limit | How far the head turns from straight ahead, in degrees |
| Lag | How much the tail and wings trail behind what the body does |

## From scripts

A script tells a Mob Motion what it cannot see for itself:

```csharp
var motion = Obj.MobMotion;
motion.LookAt(player.Position + Vector3.UnitY * 1.6f);
motion.Attack();
```

| Call | What it does |
| --- | --- |
| `Obj.AddMobMotion()` | Gives the Obj a Mob Motion |
| `LookAt(point)` | Turns the head to a point in the world, until told otherwise |
| `LookAhead()` | Looks ahead again |
| `Attack()` | Swings at what is before it |
| `Hurt()` | Flinches |
| `MoveAs(velocity)` | Walks as if it moved at this velocity, for an Obj that is moved in jumps, such as one a network places. `FollowObj()` goes back to watching the Obj |
| `LegSwing`, `ArmSwing`, `Stride`, `Bounce`, `Idle`, `LookSpeed`, `LookLimit`, `Lag` | Its settings |

To have a creature walk about by itself in a Block World, add a [[Block Mobs|Block Mob]] as well.

## With clips of your own

For a movement Mob Motion does not make, key a clip in the Model window (see **Clips made with keys** in [[Box Models]]) and add an [[Animator]] to the same Obj. While the Animator has a clip, the clip poses the model and Mob Motion waits. Clear the Animator's clip and Mob Motion carries on from standing.

## Watching it in the Model window

You can see a model move before it is in a level. Open it in the Model window and click **Motion** in the bar. Pick **Stand**, **Walk** or **Run**, and click **Attack** or **Hurt** to see those. Nothing about the model changes while you watch, and going back to **Shape** puts it at rest.

## Not done yet

- Feet placed on uneven ground.
- Mob Motion and a clip at once, as a creature that walks while a clip moves its arms.
