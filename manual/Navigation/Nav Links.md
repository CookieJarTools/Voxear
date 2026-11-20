---
order: 4
---

A **Nav Link** joins two places that walking does not: the foot of a ladder and its top, the two sides of a gap, the edge of a drop and the ground under it, a door that takes you somewhere else. Things finding their way take it as one step, and cross it as you say.

## Making a ladder

1. Make an empty Obj and stand it on the ground at the ladder's foot.
2. Choose **Add Component ▸ Navigation ▸ Nav Link**.
3. Set **End** to where the ladder's top is from the Obj: for a ladder up a wall four blocks high, a block in and four up, `1, 4, 0`. **End** is in the Obj's own space, so the link moves and turns with it.
4. Set **Kind** to **Climb**.

Turn on **Gizmos ▸ Navigation** in the Scene view to see the link as a purple arrow, one each way when it goes both ways.

## Settings

| Setting | What it does |
| --- | --- |
| End | Where it leads, from its Obj, in the Obj's own space |
| Kind | How it is crossed: **Walk** straight along it, **Jump** over it in a leap, **Climb** it slowly, or **Teleport** from one end to the other at once |
| Both Ways | On, it is taken back the other way too. Off, only from its Obj to its end, as a drop down a cliff |
| Cost | What taking it costs more than walking as far, in metres of plain ground. More keeps things off it where there is another way |

A link starts and ends at the places to stand nearest its two ends. Put each end on or just over a floor something can stand on.

### Nav Links from a script

`Obj.NavLink` is an Obj's Nav Link, null when it has none, and `Obj.AddNavLink` gives it one, with each setting above by its name, `Kind` as a `NavLinkKind`. Ways searched after a change go by it, as a drawbridge raised costs too much to take:

```csharp
Obj.NavLink?.Cost = lowered ? 1f : 1000f;
```

## Crossing a link

A Nav Agent or a Block Mob reaching a link crosses it along its line, or along an arc for a jump, whatever way it moves: a Character Controller is carried across, as on a ladder. Then it walks on.
