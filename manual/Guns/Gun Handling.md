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

## Clips

For a gun from an animation pack, its clips can hold it instead (see [[Guns from Animation Packs]]). With none set, the gun is held as every gun is.

- **Carrier**: the bone of the arms clips' skeleton that carries the gun, picked from the first arms clip set. **Auto** takes a bone named `gun_carrier`, or else the right hand. **Carrier At** and **Carrier Turn** are where the gun sits from it.
- **Rest**: the gun's own clip it stands in between actions.
- **Idle**, **Walk**, **Run** and **Sprint**: the arms clips the gun is held by as its holder stands, walks, runs and sprints. They blend by how fast the holder goes, against **Walk Speed** and **Run Speed** in metres a second, and Sprint while the holder sprints. A clip not set gives its share to the nearest one that is.
- **Fire** and **Aimed Fire**: the arms clips a shot plays, from the hip and aimed. The gun kicks as they move it, in place of its own kick. **Gun Fire**: the gun's own clip a shot plays, such as its trigger. A bolt-action works its bolt once they are over.
- **Blend**: how long a clip takes to come in and go, in seconds.

The Holding offsets still apply on top: **Hip** and **Hip Turn** move the gun from where the clips carry it, and aiming still brings the sights to the eye.

## How an action plays

Each action's **Mode** says how it plays:

- **Built-in**: as its kind's built-ins have it.
- **Based on Clip**: as built, with a clip on top of it (see below).
- **Clips**: from clips, part by part, for a gun from an animation pack.

**Speed** sets how fast a built-in action plays, in every mode.

## Basing an action on a clip

An action based on a clip from your project still plays as it always does. The clip goes on top of it:

- **Clip**: the clip, picked from the project's model files or dragged onto the field from the Project panel. **None** takes it away.
- **Start** and **End**: the part of the clip the action plays, in seconds. An **End** of 0 plays to the clip's end. That part is stretched over the action, so a slower action plays it slower.
- **Body**: how much the spine, neck, head and collarbones turn as the clip turns them.
- **Gun**: how much the gun goes where the clip's gun hand takes it, in place of the action's own lift and tilt.
- **Arms**: how much the elbows bend the way the clip bends them.
- **Blend**: how long the clip takes to come in as the action starts, and to go as it ends, in seconds.

Each weight runs from 0, not at all, to 1, as the clip does. What the clip gives is measured from its first frame, so a clip that starts from its own idle hold adds only its movement, never its stance.

The hands still hold the gun where the action has them, the fingers close round it and the hands stay out of it. So a clip made for a different gun, or for no gun at all, still holds this one.

The clip plays on the character by bone names, so its skeleton must name its bones as the character's does. Bones the character doesn't have are left as they are.

Voxear comes with no animation clips. The clips you base actions on live in your own project.

## Playing an action from clips

An action played from clips is a list of **Parts**, played one after another. Each part has:

- **Arms**: the arms clip the hands and the gun go by.
- **Gun**: the gun's own clip its parts move by, played in step with the arms clip.
- **Start** and **End**: the stretch of the clips it plays, in seconds. An **End** of 0 plays to the clip's end. The part lasts as long as that stretch, at the action's speed.
- **When**: what must be true of the gun for the part to play, such as **Chamber Filled** or **Chamber Empty**. Two parts with the same clips but a different gun clip let working the bolt throw a case out only when there is one.
- **Repeat**: how often it plays: **Once**, **Per Clip**, **Per Missing Round**, **Per Chamber**, or **Until Full**, again and again until the rounds the parts before it and its own passes put in fill the gun.
- **Marks**: what happens to the gun, and when, in seconds into the part: **Eject** throws out what is in the chamber, **Chamber** puts the next round in, **Round In** puts one round in the feed, **Open** and **Close** hold the action open and shut it, and so on.

A round-by-round reload is three parts: the first opens the bolt and puts a round in, the second repeats **Until Full**, a round each time, and the last chambers a round and shuts the bolt. Nothing happens to the gun but at its marks, so a reload stopped part of the way has put in the rounds it had marked.

## What else a handling keeps

A handling can also keep offsets on a built-in action's beats, found by their names: **Pace**, **Delay**, where the hand holds, the gun's **Tilt** and the **Grip**. And it can have actions of its own, such as an **Inspect**, with beats of their own or parts played from clips. These are written in the file as TOML. An offset on a beat the built-in no longer has is kept and does nothing.

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
