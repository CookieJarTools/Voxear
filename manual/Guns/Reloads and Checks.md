---
order: 4
---

Reloads, working the bolt and checking the ammo are done by the hands, beat by beat, worked out from the gun's kind and what is in it. Nothing is animated beforehand: each beat is a hand reaching somewhere and doing one thing there, and a part it drives moves with the hand.

```csharp
if (Input.Pressed("Reload"))
{
	holder.Reload();
}
```

- `holder.Reload()` fills the gun as its kind is filled: a magazine swapped, a stripper clip pressed in, shells put in one at a time, a revolver tipped open and each chamber loaded. It takes rounds from `holder.Spare`. A full gun isn't reloaded.
- `holder.Do(GunAction.Cycle)` works the bolt, slide or pump by hand. A bolt-action or pump is worked by itself after each shot unless `AutoCycle` is off.
- `holder.CheckMagazine()` looks at how much is left, and `holder.CheckChamber()` opens the action a little to see whether a round is chambered.
- `holder.Cancel()` stops the action where it is, as sprinting or a sudden shot should; the hands go back to the gun.

`holder.Acting` says which action is under way. The gun doesn't fire during one.

How each action goes, and actions of a gun's own such as an inspect, are set in its [[Gun Handling]].

## How much is left

`gun.Rounds` is the truth. What the player knows is another matter:

- `gun.FeedKnown` is **Exact** after loading, or after looking at a small magazine or a revolver's cylinder, and **Rough** after looking at a long magazine or belt. It goes back to **Unknown** once the player fires from the feed again.
- `gun.Estimate` is about how full it looked: Empty, Under Half, Half, Over Half or Full, or null while unknown.
- `gun.ChamberKnown` says whether they know what is in the chamber.

Show what fits your game: the estimate for a tense one, the count for an arcade one.

## What is thrown away

Spent cases fly out of the port, empty magazines drop, stripper clips are flicked off as the bolt closes over them, and an en-bloc clip pings out after its last round. Each is thrown with the gun's own velocity, falls and bounces, and goes after half a minute. A part-used magazine goes back in the pouch, and its rounds back into `Spare`.
