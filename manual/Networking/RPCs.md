---
order: 3
---

An RPC runs a behaviour's method on another machine. A player's machine asks the server to do something, as firing or opening a door, and the server tells players' machines something happened, as an explosion. The behaviour must be on an Obj with a Net Object, in a partial class.

## Asking the server

Mark a method `[ServerRpc]` and write what the server does. Voxear writes a second method beside it, with the same name and `OnServer` after it, which sends the call:

```csharp
public sealed partial class Gun : Behaviour
{
	[ServerRpc]
	private void Fire(Vector3 aim)
	{
		// Runs on the server.
		if (Physics.Raycast(Obj.WorldPosition, aim, out var hit, 100f))
		{
			Log.Info($"hit {hit.Obj.Name}");
		}
	}

	protected override void Update()
	{
		if (Obj.NetObject.IsMine && Input.Pressed("Fire"))
		{
			FireOnServer(Obj.Forward);
		}
	}
}
```

The server runs it only for the Obj's owner. Anyone else's is dropped, so a player cannot fire someone else's gun. `Net.Caller` says who asked, and `Net.CallTick` the tick they asked on.

| Setting | What it does |
|---|---|
| `AnyPlayer` | Any player may ask, not only the owner, as for a door anyone opens. |
| `PerSecond` | The most times a second each player may ask; the rest are dropped. |
| `Unreliable` | It may be lost, but goes sooner and is never held up by others. |

```csharp
[ServerRpc(AnyPlayer = true, PerSecond = 2)]
private void Open()
{
	open = !open;
}
```

## Telling players

Mark a method `[ClientRpc]` and write what each player's machine does. On the server, call it with `OnClients` after its name for every player who has the Obj, a host's own player too, or with `OnClient` and a player for one:

```csharp
public Prefab Blast;

[ClientRpc(Unreliable = true)]
private void Exploded(Vector3 at)
{
	Blast.Instantiate(at, Quaternion.Identity);
}

// On the server:
ExplodedOnClients(Obj.Position);
ExplodedOnClient(player, Obj.Position);
```

A player whose machine does not have the Obj, because it is out of their range, does not hear of it. Anything a player who comes back in range needs to know belongs in a [[Sharing Objs#Replicated properties|replicated property]] instead.

## What RPCs carry

Arguments can be `bool`, `int`, `long`, `float`, `double`, `string`, `byte[]`, `Vector2`, `Vector3`, `Quaternion`, enums, `Obj` and `NetPlayer`. An `Obj` goes as the shared Obj it is, and reads as `Obj.None` on a machine that does not have it.

RPCs arrive in the order they were sent, unless they are `Unreliable`. Arguments come to at most 8 KB, and 1000 bytes for one that may be lost.

## Limits

However a game's own RPCs are set, the server takes at most 120 a second from each player, with twice that at once, and drops the rest. A player's machine is never trusted: check on the server that what they ask for makes sense, as that they have ammunition left.
