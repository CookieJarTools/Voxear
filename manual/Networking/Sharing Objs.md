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
- **Range**: how far, in metres, a player's own Objs may be from it for it to be sent to them. Further away, it is taken away from their machine, and made again when they come back in range. 0 sends it to everyone, but for one of a crowd (below).

Updates are small. Each player's update is anchored where they are, so an Obj within about 300 m of them goes as how far it is from them, to a centimetre, and one turned only about up, as characters are, as its heading alone: about 9 bytes where it took 16.

## Crowds

An Obj with a **Nav Agent**, a **Block Mob** or a **Crowd Member** is one of a crowd, and goes to each player by how near it is to them, as **Project Settings ▸ Crowds** sets the distances:

- within **Near** (40 m), every tick;
- out to **Middle** (150 m), 20 times a second;
- beyond, 5 times a second, their machine moving it smoothly between;
- beyond **Records beyond** (250 m), not at all, unless it has a range of its own: there it would be a record on the server anyway (see [[Crowds]]).

A player's machine animates every character from how it moves, so poses are never sent. Within the usual budget a player can follow over a hundred characters near them every tick, or a thousand spread over the distances.

## Dimensions

Each player has the dimension their first Obj is in, and the one the Portal open for them leads into (see [[Dimensions]]). A shared Obj is sent to a player only while it is in a dimension they have, and its range is measured in that dimension: an Obj at the same numbers in another dimension is nowhere near. Every player has the level's own dimension, so an Obj there with a range of 0 still goes to everyone.

When an Obj goes into another dimension on the server, through a Portal or moved by a script, each player who has it sees it go as it reaches the Portal. A player who does not have the dimension it went into has it taken away. An Obj a scene placed is only let be on their machine, where it was, and found again when it comes back; one made from a prefab is made again.

The server decides which dimension each Obj is in. Players' machines never take an Obj the server moves through a Portal of their own. With [[Shared Authority|shared authority]], a player's machine takes its own Objs through, and the server checks that there is a Portal by the Obj into that dimension. If there is not, the Obj is put back where the server has it.

## Inside other Objs

When a script on the server puts a shared Obj inside another with `SetParent`, as a boat takes a player aboard, each player's machine puts its copy inside the same Obj, as long as that Obj is shared too. When the server puts it back at the top, it goes back to the top on every machine. A player's own predicted Obj follows when the server puts it right, so a player taken aboard rides the boat on their own machine as well.

Give the Obj it goes inside a Net Object. An Obj inside one that is not shared stays inside whatever each player's own scene put it in. The server can't say which Obj that is.

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

A `Vector3` that is a place in the world, rather than a direction or a size, is marked `[WorldPosition]` as well: `[Replicated, WorldPosition] public partial Vector3 Rally { get; set; }`. It is sent measured from the level's middle, so a player whose world's middle has moved with them (see [[Floating Origin]]) reads it where it is in their world. An RPC's argument is marked the same way.

## Far from the middle

Every Obj's position goes between machines measured from the level's middle, a millimetre's steps out to half a million kilometres either way. With the [[Floating Origin]] switched on, each player's machine moves its own middle along with its camera, and the server keeps the level's; neither needs to know where the other's is.
