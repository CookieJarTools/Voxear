---
order: 2
---

A **Gun Holder** holds a gun in front of its character's [[First-Person View]], both hands on it. Add one to the same Obj as the view, with **Add Component ▸ Guns ▸ Gun Holder**. It holds the first Gun under its Obj.

```csharp
public sealed class Soldier : Behaviour
{
	protected override void Update()
	{
		var holder = Obj.GunHolder;
		holder.Aiming = Input.Held("Aim");
	}
}
```

## The hip and the sights

Not aiming, the gun is held at the **Hip**: where the grip goes from the camera, right, up and forward, its stock near the shoulder; a handgun is held further out. **Hip Turn** cants it. Aiming raises the gun until its sights lie on the camera's line at the gun's eye relief, over **Aim Time**. `holder.Aimed` says how far up it is, from 0 to 1, for narrowing the view as it comes up.

When the arms can't reach the gun where it is held, it comes nearer the body until they can.

## Life on top

Everything below happens on springs, so it settles rather than stops:

- **Sway**: the gun lags the view as it turns, rolling a little as it goes.
- **Lag**: the gun trails the body's moves.
- **Bob**: a kick at each footfall, left and right in turn, from the Motion Matcher's feet.
- **Breathing**: a slow rise and fall, and a faint tremor.

**Sway**, **Bob** and **Breathing** scale each, 1 as the gun's weight has it and 0 not at all. Aiming steadies them all. A gun held by its pack's clips has each as much as its handling says on top of them, and **Base Only** shows such a gun as its clips alone have it, with nothing on top (see [[Guns from Animation Packs]]).

## Walls

With **Wall Check** on, the gun pulls back when its barrel would go into a wall, and lifts to point up when it would go further. It doesn't fire while it is held up like that.
