---
order: 2
---

**Smoke** fills the space round its Obj, as a smoke grenade does: it spreads out from the Obj, pours through doorways and round corners, and stays out of walls. Make one with the Hierarchy's **+ ▸ Smoke**, or add one to any Obj with **Add Component ▸ Effects ▸ Smoke**. It fills straight away in the Scene view too, and fills again whenever you move it or change it.

## How it fills

Smoke fills as much space as a ball of its **Size** would hold, nearest first, so in the open it's a ball and in a room it fills the room. What doesn't fit inside pours out of the doors and windows. Anything with a Collider that stays put stops it, however thin. Objs that physics moves, such as crates, and triggers don't.

**Cell Size** is how fine the smoke's cells are. Finer cells follow walls more closely, but a smoke has 32 cells along each side, so finer cells also reach less far. Half a metre or less suits a grenade.

## How it looks

- **Density**: how thick it is. Higher hides more.
- **Colour**: how much of the light it scatters. Grey for smoke, white for steam, dark for soot.
- **Spread Time**: how many seconds it takes to spread out from its Obj to its full size.
- **Lasts** and **Disperse Time**: how long it stays, and how long it then takes to disperse. As it disperses it breaks up from its thinner swirls into wisps, rises and drifts apart, and the wisps thin away to nothing. A Lasts of 0 keeps it for ever. Changing either in the editor starts the smoke again, so you can watch it.
- **Seed**: two smokes that are alike but for their Seed swirl differently.

The sun lights it through itself, so its far side is darker, and objects in front of it cast soft shadows into it. Lamps light it round themselves, so a torch in the smoke glows. It takes the light round it from the sky and bounced light, and it sits in the scene's fog as far off as it is.

From inside, you can see a metre or two through it, with thicker and thinner swirls drifting past.

## Smoke grenades

A grenade's smoke waits for its fuse. Make a prefab of the canister with a Rigidbody, a Collider and a Smoke, and turn the Smoke's **Play On Start** off. It then shows nothing until your code plays it:

```csharp
public sealed class SmokeGrenade : Behaviour
{
	[SerializeField] private float fuse = 1.5f;

	protected override void Update()
	{
		if (fuse <= 0f)
		{
			return;
		}
		fuse -= Time.Delta;
		if (fuse <= 0f)
		{
			Obj.Smoke.Play();
		}
	}
}
```

Throw it by placing the prefab at the hand and giving its Rigidbody a velocity:

```csharp
var grenade = smokeGrenade.Instantiate(hand.WorldPosition, hand.WorldRotation);
grenade.Rigidbody.Velocity = aim * 14f;
```

While the game runs, a smoke spreads out from where its Obj was when it started, and stays there, however far the canister rolls after. Started in the ground, as a canister lying on it is, it starts from just above. **Play** again starts it over from where the canister is now. **Disperse** starts it dispersing at once, however long it was to last. **IsPlaying** stays true until it has dispersed, so the grenade can destroy itself then.

A new smoke takes a few frames to fill, so one going off never stalls the game. With Play On Start on, as it is for a smoke placed in a level, it starts as soon as it appears.

## Shooting through it

Shots and blasts cut holes in smoke, which fill back in:

- **A shot** cuts a tunnel half a metre wide along its line through every smoke it passes through. Looking down it, you see through the smoke; it fills back in over about a second and a half.
- **A blast** clears a ball round it, all but a trace, pushing the smoke into a thicker shell just outside it. It fills back in slowly at first, then faster, over about five seconds.

Holes come from your game's own code, at the moment it fires:

```csharp
// A gun's shot, through any smoke on the way to what it hit.
Smokes.Shoot(muzzle, hit.Point);

// A grenade going off.
Smokes.Blast(Obj.WorldPosition, 4f);
```

`Smokes.SeeThrough(from, to)` says how much of what is at one point shows from another through the smoke between: 1 with none in the way, near 0 hidden. It counts the smoke as thick as it's drawn, as far as it has spread, as it thins away, and with its holes. Use it so enemies can't see the player through smoke but can down a fresh bullet hole:

```csharp
var seen = Smokes.SeeThrough(Obj.WorldPosition, player.WorldPosition) > 0.3f;
```

## What's coming

The smoke drifting in the wind, and spreading again when walls change, as a door opening.
