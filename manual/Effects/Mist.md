---
order: 3
---

**Mist** leaves a trail of fog behind its Obj as the Obj moves, as a wizard dashing in a cloud of mist does. Make one with the Hierarchy's **+ ▸ Mist**, or add one to any Obj with **Add Component ▸ Effects ▸ Mist**. While it's **Emitting**, the trail follows the Obj wherever it goes. Each piece of trail widens, rises and frays into wisps as it ages, then thins away. Stop emitting and what it left lingers a moment, then is gone.

## How it looks

- **Width**: how wide the trail is where it leaves the Obj, in metres. It widens to twice this as it thins.
- **Density**: how thick it is. Higher hides more.
- **Colour**: how much of the light it scatters. Pale blue for a magical mist, grey for dust, white for steam.
- **Linger**: how many seconds each piece of trail lasts before it's gone.
- **Seed**: two mists that are alike but for their Seed swirl differently.

It's lit as [[Smoke]] is: by the sun through itself, by lamps round it, and by the sky and bounced light.

Standing still while emitting, an Obj gathers a cloud of mist round it.

## A dash in a cloud of mist

Hide the body with `Obj.Visible`, leave mist behind, and push the Character Controller fast for a moment:

```csharp
public sealed class MistDash : Behaviour
{
	[SerializeField] private float speed = 20f;
	[SerializeField] private float length = 0.35f;

	private float left;
	private Vector3 way;

	/// <summary>Rushes the way the player aims, along the ground: never up or down.</summary>
	public void Dash(Vector3 aim)
	{
		way = Vector3.Normalize(new Vector3(aim.X, 0f, aim.Z));
		left = length;
		var mist = Obj.Mist;
		mist.Emitting = true;
		Obj.Visible = false;
	}

	protected override void Update()
	{
		if (left <= 0f)
		{
			return;
		}
		var controller = Obj.CharacterController;
		controller.Move((way * speed - Vector3.UnitY) * Time.Delta);
		left -= Time.Delta;
		if (left <= 0f)
		{
			var mist = Obj.Mist;
			mist.Emitting = false;
			Obj.Visible = true;
		}
	}
}
```

Give the Obj a Mist with **Emitting** off, so it only leaves mist while dashing. `Obj.Visible = false` hides the Obj's meshes and models, and those of every Obj under it. Its lights, effects, colliders and scripts carry on.

`Smokes.SeeThrough(from, to)` counts mist as well as smoke, so enemies that check it lose sight of a wizard inside the trail.
