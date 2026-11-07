---
order: 5
---

A player's machine shows the server's world a moment in the past: the time it takes the server's updates to arrive, and a little more to move smoothly between them. A player who aims true at a running target aims where it was on their screen. By the time their shot reaches the server, the target has run on, and a ray cast on the server as the world is now would miss.

## Rays as a player saw the world

`Net.RaycastAsSeenBy` casts a ray on the server as a player's machine showed the world. Use it to check their shots, in a [[RPCs|ServerRpc]]:

```csharp
[ServerRpc]
private void Fire(Vector3 aim)
{
	if (Net.RaycastAsSeenBy(Net.Caller!.Value, Obj.WorldPosition, aim, out var hit, 200f))
	{
		hit.Obj.GetBehaviour<Health>()?.Hurt(20);
	}
}
```

The server keeps where every shared Obj was on each tick, for a second. Each player's input says how far in the past their machine showed the world when they pressed. For the ray, the colliders of shared Objs it comes near, and their children's, are put back where they were then. The ray is cast, and they are put back where they are.

## What it does not do

- **It goes back no more than 0.2 seconds.** A player with a slow connection cannot hit someone who has long since got behind a wall.
- **The shooter's own Objs stay where they are**, as their machine predicts them in the present.
- **Colliders count, not Character Controllers.** Give a character colliders to be hit with, as on its body and head.
- **A host's own player** sees the world as it is, so their rays are cast as `Physics.Raycast` casts them.
