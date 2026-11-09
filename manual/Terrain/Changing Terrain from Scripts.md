---
order: 9
---

Scripts change the ground while the game runs: a shell blows a crater, an engineer digs a gun pit flat, a cart paints mud along its track, a charge cuts a way into a bunker, a fire leaves the grass burnt away. Physics stands things on the new ground at once and the terrain is drawn as it is now, from the tiles the change reached and no others.

```csharp
public MaterialAsset Dirt;

private void Landed(Vector3 at)
{
	Terrain.Crater(at, radius: 3f, depth: 1.2f, walls: Dirt);
}
```

A change from a script is made straight to the ground the terrain has, and the terrain's [[Layers]] are left as they are: they are what the editor builds the ground from, and a built game has the ground alone. Play mode in the editor works the same way, and when play stops the terrain is back as you made it.

## Changes to the ground

Every call takes a point in the world and a radius in metres, and changes every terrain the radius reaches, easing off towards its edge. Each returns whether it reached a terrain.

| Call | What it does |
| --- | --- |
| `Terrain.Raise(point, radius, height)` | Raises the ground by `height` at the middle |
| `Terrain.Lower(point, radius, depth)` | Lowers the ground by `depth` at the middle |
| `Terrain.Flatten(point, radius, height)` | Brings the ground to `height` in the world, for a trench floor or a gun pit |
| `Terrain.Crater(point, radius, depth, walls)` | A bowl `depth` deep with its rim thrown up round it. The grass in it is cleared, the plants in it are taken away, and the surface drawn with `walls` is laid on its walls |
| `Terrain.Paint(point, radius, surface, strength)` | Lays the surface drawn with `surface` over the ground, as much as `strength`, 0 to 1, at the middle |
| `Terrain.CutHole(point, radius)` | Cuts the ground away: things fall through and rays pass, as through a [[Holes|hole]] cut in the editor |
| `Terrain.FillHole(point, radius)` | Fills the holes there again |
| `Terrain.ClearGrass(point, radius)` | Clears every kind of [[Grass]] there |
| `Terrain.GrowGrass(point, radius, amount)` | Grows every kind of grass there towards `amount`, 1 for fully |

A terrain's surfaces are named by their materials: give a behaviour a `MaterialAsset` field and pick the material in the Inspector, the same one the surface has in the Terrain's list (see [[Surfaces]]). A crater whose `walls` the terrain has no surface for leaves the ground's surfaces as they were, and a paint of one does nothing and returns false.

Raising and lowering move the ground up and down only. A crater has no overhangs and a raised mound no caves: digging under the ground is for marching cubes, which come later.

## Edits as values

Each call makes a `TerrainEdit` and applies it. Make one yourself to keep it, pass it on or apply it later:

```csharp
var blast = TerrainEdit.Crater(hit.Point, 4f, 1.5f, Dirt);
Terrain.Apply(blast);
```

| Edit | Made by |
| --- | --- |
| Raise | `TerrainEdit.Raise(point, radius, height)`, less than nothing to lower |
| Crater | `TerrainEdit.Crater(point, radius, depth, walls)` |
| Flatten | `TerrainEdit.Flatten(point, radius, height)` |
| Paint | `TerrainEdit.Paint(point, radius, surface, strength)` |
| Holes | `TerrainEdit.Holes(point, radius, cut)`, `cut` false to fill |
| Grass | `TerrainEdit.Grass(point, radius, amount)` |

A `TerrainEdit` is a record: its `Kind`, `Point`, `Radius`, `Amount`, `Surface` and `Cut` can be read, and `with` makes another from it, as `blast with { Radius = 6f }`.

## The same ground on every machine

The same edits, applied in the same order to the same terrain, give exactly the same ground on every machine, down to the last bit of every height. Nothing in them depends on the machine: no random numbers, no time, and none of the maths that can come out differently on another processor. So a networked game sends what makes an edit and lets every player make it, rather than sending the ground.

Send the edit's parts in a [[RPCs|client RPC]], which a host runs for its own player too. A dedicated server has no player, so it makes the edit itself as well, for its physics:

```csharp
public MaterialAsset Dirt;

[ClientRpc]
private void Exploded(Vector3 at, float size)
{
	Terrain.Crater(at, size, size * 0.4f, Dirt);
}

// On the server, where the shell landed:
if (!Net.IsHost)
{
	Terrain.Crater(at, size, size * 0.4f, Dirt);
}
ExplodedOnClients(at, size);
```

## Players who join later

A player who joins after the shelling started has the level as it was built. `Terrain.CaptureEdits()` gives every change scripts made to the level's terrains, and `Terrain.ApplyEdits(bytes)` makes them on another machine: it puts each terrain back as it was built first, so it can be called again to catch up.

The bytes are as few as say it. They are the edits themselves, about 20 bytes each, so ten craters are 200 bytes and a thousand under 20 KB. Past a few thousand, they are instead the tiles the edits changed, each kept as only how it differs from the ground as built, compressed: at most about 100 KB for a tile torn up all over, however many more shells land on it. Edits set heights to the nearest millimetre or so, which no one sees, so that a tile's changes stay small.

An RPC carries at most 8 KB, so send larger edits in pieces:

```csharp
private readonly List<byte> arriving = [];

// On the server:
Net.PlayerJoined += player =>
{
	var edits = Terrain.CaptureEdits();
	for (var at = 0; at < edits.Length; at += 8000)
	{
		var last = at + 8000 >= edits.Length;
		GroundOnClient(player, edits[at..Math.Min(at + 8000, edits.Length)], last);
	}
};

[ClientRpc]
private void Ground(byte[] piece, bool last)
{
	arriving.AddRange(piece);
	if (last)
	{
		Terrain.ApplyEdits([.. arriving]);
		arriving.Clear();
	}
}
```

Send the edits before the live RPCs that follow, on the same Obj, so they arrive first.

## Saves

A save of the scene (see [[Saving the Game]]) keeps the ground scripts changed, with nothing to mark: it holds the same bytes `CaptureEdits` gives, and `Save.LoadScene` makes the changes again on the level loaded afresh.
