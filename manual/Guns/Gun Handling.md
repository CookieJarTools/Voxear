---
order: 6
---

A **Gun Handling** is how one gun is held and how its actions go, kept in a `.gunhandling` file that the Gun picks in its **Handling** field. Guns that share a design can share one. A Gun with no handling is held and handled as its kind's built-ins have it.

Make one with **+ ▸ New Gun Handling** in the Project panel, and select it to work on it in the Inspector.

## Gun Kind

**Gun Kind** is the kind of gun the handling is for. The Inspector then lists only the built-in actions that kind has: a pump shotgun has a pump to work, a pistol has none. With **Any**, every built-in action is listed.

## Holding

Holding is offsets on how every gun is held, all 0 in a new handling:

- **Hip** and **Hip Turn**: where the gun is held at the hip, from the eyes.
- **Aim**, **Aim Turn** and **Cant**: where the gun is held at the eye, in the sights' own space, and how far its top leans right.
- **Shoulder**: where a long gun's butt sits on the shoulder.
- **Grip Hand**, **Support Hand** and their turns: where each palm holds at rest, on the grip and under the fore-end.

Places are in metres, turns in degrees.

## Basing an action on a clip

Each action can be based on a clip from your project, such as a reload from an animation pack. The action still plays as it always does. The clip goes on top of it:

- **Clip**: the clip, picked from the project's model files or dragged onto the field from the Project panel. **None** takes it away.
- **Start** and **End**: the part of the clip the action plays, in seconds. An **End** of 0 plays to the clip's end. That part is stretched over the action, so a slower action plays it slower.
- **Body**: how much the spine, neck, head and collarbones turn as the clip turns them.
- **Gun**: how much the gun goes where the clip's gun hand takes it, in place of the action's own lift and tilt.
- **Arms**: how much the elbows bend the way the clip bends them.
- **Blend**: how long the clip takes to come in as the action starts, and to go as it ends, in seconds.

Each weight runs from 0, not at all, to 1, as the clip does. What the clip gives is measured from its first frame, so a clip that starts from its own idle hold adds only its movement, never its stance.

The hands still hold the gun where the action has them, the fingers close round it and the hands stay out of it. So a clip made for a different gun, or for no gun at all, still holds this one.

The clip plays on the character by bone names, so its skeleton must name its bones as the character's does. Bones the character doesn't have are left as they are.

**Speed** sets how fast a built-in action plays, and with it the clip.

Voxear comes with no animation clips. The clips you base actions on live in your own project.

## What else a handling keeps

A handling can also keep offsets on a built-in action's beats, found by their names: **Pace**, **Delay**, where the hand holds, the gun's **Tilt** and the **Grip**. And it can have actions of its own, such as an **Inspect**, with beats of their own. These are written in the file as TOML. An offset on a beat the built-in no longer has is kept and does nothing.

Scripts play an action of the gun's own with a typed reference:

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

## Changes while playing

The Inspector writes the file as each change is made. A game playing in the editor reads it again at once: holding changes on the next frame, and an action changes the next time it starts.
