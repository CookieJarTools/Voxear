---
order: 6
---

A **Gun Handling** is how one gun is held and how its actions go, kept in a `.gunhandling` file that the Gun picks in its **Handling** field. Guns that share a design can share one. A Gun with no handling is held and handled as its kind's built-ins have it.

Make one with **+ ▸ New Gun Handling** in the Project panel, with the **+** in the [[The Gun Editor|Gun Editor]], or with **Open in Gun Editor** on a Gun that has none. Work on it in the Gun Editor rather than by hand.

## Poses

Poses are offsets on how every gun is held, all nothing in a new handling:

- **Hip** and **Hip Turn**: where the gun is held at the hip, from the eyes.
- **Shoulder**: where a long gun's butt sits on the shoulder.
- **Aim**, **Aim Turn** and **Cant**: where the gun is held at the eye, in the sights' own space, and how far its top leans right.
- **Grip Hand**, **Support Hand** and their turns: where each hand holds at rest, on the grip and under the fore-end.
- **Rest Grip** and **Support Grip**: how the fingers close at rest, instead of the usual.
- **Work** and **Work Turn**: where actions bring the gun before the eyes.
- **Return Pace**: how quickly a hand goes back to its hold after an action.
- **Left Pouch** and **Right Pouch**: where spare magazines and clips are taken from.

## Built-in actions

Reload, Cycle, Check Magazine and Check Chamber are built into each kind of gun as a run of beats. A handling keeps offsets on them, never copies, so improvements to the built-ins still reach your guns:

- On the whole action: **Speed**, **Lift** (how far the gun comes up before the eyes) and **Work**.
- On each beat, found by its name: **Pace**, **Delay**, where the hand holds (**Hand**, **Hand Turn**), the gun's **Tilt**, and the **Grip**.

An offset on a beat the built-in no longer has is kept and does nothing. The Gun Editor lists these and can remove them.

## Actions of the gun's own

When offsets aren't enough, **Make Editable Copy** turns a built-in action into an action of the gun's own that takes its place: every beat, its timing, goal, grip, the part it drives or carries, and what it does to the gun, all yours to change. Beats still play only in the states they did, and repeat as they did (once per clip, per missing round or per chamber).

An action of its own can also stand alone, such as an **Inspect**. Scripts play it with a typed reference:

```csharp
namespace Soldiers;

public sealed class Soldier : Behaviour
{
	public GunHandlingAction Inspect;

	protected override void Update()
	{
		if (Input.Pressed("Inspect"))
		{
			Obj.GunHolder.Do(Inspect);
		}
	}
}
```

The Inspect field offers the actions of the project's handlings. `holder.Doing` says which action is under way, as it was asked for.

## Saving and changes while playing

The Gun Editor writes the file as each change is made. A game playing in the editor reads it again at once: poses change on the next frame, and an action changes the next time it starts.
