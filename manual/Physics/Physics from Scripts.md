---
order: 2
---

Scripts reach an Obj's physics the way they reach its other components, through the Obj: `Obj.Collider`, `Obj.Rigidbody` and `Obj.CharacterController`. The `Physics` class answers questions about the whole world, such as what a ray meets.

## Pushing a Rigidbody

`AddForce` pushes a body. A force spreads over the next physics step, so give it in `FixedUpdate`, every step you want it to push:

```csharp
namespace MyGame;

public sealed class Thruster : Behaviour
{
	public float Strength = 40f;

	protected override void FixedUpdate()
	{
		Obj.Rigidbody.AddForce(Vector3.UnitY * Strength);
	}
}
```

The second argument says how the push is measured, as in Unity:

| ForceMode | What it does |
| --- | --- |
| Force | A push in newtons, spread over the step. A heavier body moves less. This is what you get unless you say otherwise |
| Acceleration | The same, but whatever the body weighs |
| Impulse | A kick all at once, such as a jump or a hit. A heavier body moves less |
| VelocityChange | A kick all at once, whatever the body weighs |

A kick happens once, so it can go anywhere, such as in `Update` when a key is pressed:

```csharp
if (Input.Pressed("Jump"))
{
	Obj.Rigidbody.AddForce(Vector3.UnitY * 5f, ForceMode.VelocityChange);
}
```

`AddForceAtPosition` pushes at a point, so the body turns as well. `AddTorque` turns it without moving it. `AddExplosionForce` throws it away from a point, harder the closer it is, which suits an explosion:

```csharp
foreach (var caught in Physics.OverlapSphere(blast, 6f))
{
	if (caught.Rigidbody.IsValid)
	{
		caught.Rigidbody.AddExplosionForce(800f, blast, 6f, upwardsModifier: 1f);
	}
}
```

You can also set `Velocity` and `AngularVelocity` directly, and read them back. Set the Obj's position and the body goes straight there, as in Unity.

A body you give a Rigidbody in a script can be pushed straight away, in the same `Start`. Give it a Collider too, or physics has no shape to move.

## Joining bodies

`Obj.AddJoint` gives an Obj a joint, held to another Obj's Rigidbody or, without one, to the world:

```csharp
var hinge = Obj.AddJoint(JointType.Hinge);
hinge.Anchor = new Vector3(-0.5f, 0f, 0f);
hinge.SetLimits(0f, 90f);
```

`ConnectedBody` changes what it holds to, and `ClearLimits` lets it turn all the way round again. Changing a joint remakes it where its Objs are at that moment.

## Hearing what touches what

A behaviour hears about collisions on its own Obj by overriding these, as in Unity:

```csharp
public sealed class Crate : Behaviour
{
	public Sound Thud;

	protected override void OnCollisionEnter(Collision collision)
	{
		Audio.Play(Thud, at: collision.Point);
	}

	protected override void OnTriggerEnter(Collider other)
	{
		Log.Info($"{other.Obj.Name} came in");
	}
}
```

| Method | When |
| --- | --- |
| OnCollisionEnter | Its collider first touches another. One of the two needs a Rigidbody |
| OnCollisionExit | They stop touching |
| OnTriggerEnter | Something enters its trigger, or it enters another Obj's trigger |
| OnTriggerExit | It leaves again |

They come straight after the physics step, before the next `FixedUpdate`. `collision.Obj` is the other Obj, `collision.Point` is where they touched, and `collision.Normal` points the way the other Obj pushes this one: up, for a crate landing on the floor. A disabled behaviour is told too, so it can enable itself when something touches it.

A body that comes to rest on another still counts as touching it, so you won't get `OnCollisionExit` just because it went to sleep.

## Asking what is where

`Physics.Raycast` sends a ray out and tells you the first collider it meets:

```csharp
if (Physics.Raycast(Obj.WorldPosition, -Vector3.UnitY, out var hit, 2f))
{
	Log.Info($"{hit.Obj.Name} is {hit.Distance:0.0} m below");
}
```

`hit.Point` is where the ray met it, and `hit.Normal` is the way that surface faces. A ray that starts inside a collider passes out of it without meeting it, so a ray from the middle of the player doesn't find the player.

`Physics.Linecast` asks the same between two points. `Physics.OverlapSphere` lists the Objs whose colliders touch a sphere, and `Physics.CheckSphere` only says whether there are any.

These questions see colliders where the last physics step left them, and they pass over triggers. Pass `hitTriggers: true` to find triggers as well. That is the one place Voxear differs from Unity, where rays find triggers unless you tell them not to.

Each question also takes a layer mask, as in Unity, and then only looks at Objs on those layers. Leave it out to look at every layer. `LayerMask.GetMask` makes one from the layers' names:

```csharp
var ground = LayerMask.GetMask("Ground", "Platforms");
var grounded = Physics.Raycast(Obj.WorldPosition, -Vector3.UnitY, out var hit, 1.1f, ground);
```

A mask is a bit for each layer, so `1 << 8` is layer 8 alone and `~0` is every layer. A ray finds Objs on its mask's layers whether or not those layers collide with anything.

`Physics.Gravity` is which way and how hard everything falls. It starts at 9.81 metres per second per second, downwards. `Physics.GravityAt(position)` says which way and how hard things fall at a point: `Physics.Gravity`, the same everywhere.

## Walking a character

A Character Controller moves only when your code moves it. `SimpleMove` is the quickest way: give it a speed across the ground, once a frame, and it adds gravity for you:

```csharp
public sealed class Player : Behaviour
{
	public float Speed = 4f;

	protected override void Update()
	{
		var walk = Input.Vector2("Move") * Speed;
		Obj.CharacterController.SimpleMove(new Vector3(walk.X, 0f, walk.Y));
	}
}
```

For jumping, or anything else that needs full control, use `Move`. It takes how far to move this frame, in metres, and adds nothing, so keep your own falling speed:

```csharp
var controller = Obj.CharacterController;
if (controller.IsGrounded)
{
	falling = Input.Pressed("Jump") ? 5f : -1f;
}
falling += Physics.Gravity.Y * Time.Delta;
controller.Move((across + Vector3.UnitY * falling) * Time.Delta);
```

`IsGrounded` says whether it stood on walkable ground after its last move, and `Velocity` how fast it really went once walls and the ground stopped what they could. `GroundNormal` is the way the ground under it faces.

## Pushing a character

A river carries a character wading it, and `AddPush` shoves one: a blast, a gust or a blow. The push is added to every move it makes and dies away over a second or so. Shoves add up:

```csharp
var away = Vector3.Normalize(player.WorldPosition - blast);
player.CharacterController.AddPush(away * 8f + Vector3.UnitY * 3f);
```

`Pushed` is how fast it is carried and shoved together now. `WaterPush` is how much flowing water carries it, 1 by default: 0 for a character that stands its ground in any current.

`Up` is which way the character stands, against gravity: straight up, unless a game turns `Physics.Gravity` another way. To jump and fall whichever way gravity pulls, keep the falling speed along `Up` and take it from `Physics.GravityAt`:

```csharp
var up = controller.Up;
if (controller.IsGrounded)
{
	falling = Input.Pressed("Jump") ? 5f : -1f;
}
falling += Vector3.Dot(Physics.GravityAt(Obj.WorldPosition), up) * Time.Delta;
controller.Move((across + up * falling) * Time.Delta);
```
