---
order: 2
---

The server shares Objs with its players. On their machines, a shared Obj is the server's copy's double: it moves as the server's moves, and its replicated properties change as the server's do.

## Sharing an Obj

Give an Obj a **Net Object** (Add Component ▸ Networking ▸ Net Object). An Obj with one is shared when it is:

- **placed in a scene**: each player's machine finds its own, where the same scene placed it;
- **the root of a prefab** made on the server with `Instantiate`: each player's machine makes its own from the prefab.

When the server destroys a shared Obj, each player's copy goes too. Objs that physics moves on the server are moved by the server alone on players' machines.

## Who owns what

A shared Obj can belong to a player. Set its owner on the server:

```csharp
Net.PlayerJoined += player =>
{
	var soldier = soldierPrefab.Instantiate(spawn.WorldPosition, spawn.WorldRotation);
	var net = soldier.NetObject;
	net.Owner = player;
};
```

A player's own Objs are always sent to them, and where their first one is counts as where they are. `NetObject.IsMine` says whether an Obj belongs to this machine's player.

## Priority and range

There is only so much room in each update a player gets. What matters most goes first, and what does not fit goes next time, gaining in priority as it waits, so nothing is left behind for long. Nearer Objs count for more than far ones.

- **Priority**: 1 as usual, 2 to be sent twice as often as others when there is not room for everything.
- **Range**: how far, in metres, a player's own Objs may be from it for it to be sent to them. Further away, it is taken away from their machine, and made again when they come back in range. 0 sends it to everyone.

## Smooth movement

A player's machine shows shared Objs a moment in the past, about two ticks behind the newest update, and longer when updates arrive unevenly. That way it always has an update on either side to move smoothly between, even when one is late or lost. Positions are sent to about a millimetre.

## Replicated properties

A behaviour's property can be shared too. Mark a partial property `[Replicated]`, on a behaviour of an Obj with a Net Object:

```csharp
public sealed partial class Health : Behaviour
{
	[Replicated]
	public partial int Points { get; set; }
}
```

Voxear writes the property for you. Set it on the server, and it changes on every player's machine shortly after. Only what changed is sent, and only when it changes. `bool`, `int`, `long`, `float`, `double`, `string`, `Vector2`, `Vector3`, `Quaternion`, enums, `Obj` and `NetPlayer` can be replicated. An `Obj` reads as `Obj.None` on a machine that does not have it yet.

Set replicated properties on the server only: a player's machine takes them from the server, and its own changes are not sent anywhere.
