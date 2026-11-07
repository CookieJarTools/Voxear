---
order: 6
---

By default the server decides everything, and players' machines only send what they press. That makes cheating hard, which matters in a game played against others. In a game played together, it matters less than nothing ever waiting on the server. There, each player's machine can run their own Objs.

```toml
[network]
authority = "shared"
```

## Players' own Objs

With shared authority, the machine of an Obj's owner runs it: its behaviours move it, its physics simulates it. Each tick it tells the server how the Obj is, and sends its replicated properties as they change. The server takes its word and passes it on to everyone else. Objs no one owns stay the server's.

`NetObject.HasAuthority` says whether this machine decides what an Obj does. Move it, push it and set its replicated properties only where it is true:

```csharp
protected override void Update()
{
	if (!Obj.NetObject.HasAuthority)
	{
		return;
	}
	Obj.Position += Obj.Forward * speed * Time.Delta;
	Fuel -= Time.Delta;
}
```

A Rigidbody is simulated only where its Obj's authority is. Elsewhere it is held still and moved as it is told.

## Changing hands

The server can give any Obj to anyone with `NetObject.Owner`. A player can ask for one no one owns, as to pick it up, with `NetObject.Claim()`, and the server gives it to them. Whoever an Obj changes hands to, every machine hears of it. When a player leaves, the server takes back everything that was theirs.

## What it costs

A player's machine is trusted with their own Objs, so a player could move theirs anywhere. Keep what matters to a fair game, as scores and damage, on Objs the server owns, and check what players ask for in [[RPCs]].
