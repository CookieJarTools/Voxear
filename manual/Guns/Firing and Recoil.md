---
order: 3
---

`holder.Fire` fires the gun when it is ready: a round chambered, no faster than its **Fire Rate**, and not held up against a wall. It gives back where the shot went, from the eye along the view, spread less the higher the gun is raised:

```csharp
protected override void Update()
{
	var holder = Obj.GunHolder;
	if (Input.Held("Fire") && holder.Fire(out var shot)
		&& Physics.Raycast(shot.Origin, shot.Direction, out var hit, 400f))
	{
		Log.Info($"Hit {hit.Obj}");
	}
}
```

Call it every frame the trigger is held for automatic fire, or only when it is pressed for one shot at a time.

## Recoil

Each shot kicks in four ways at once:

- **The view climbs** in a pattern of the gun's own, wandering left and right, the first shot hardest. Once the shots stop, it comes back down, all but what the player pulled down themself. A bolt-action doesn't come back, as its shooter takes aim again for each shot.
- **The gun kicks** up and back about its grip, rolling the other way each shot.
- **The camera jolts** and springs back.
- **The action works**: a self-loader's bolt or slide flies back and home, and throws out the case.

Heavier guns kick less. **Recoil** on the Gun scales it all.
