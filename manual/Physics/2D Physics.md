---
order: 3
---

For a 2D game, such as a platformer or a top-down shooter, use the 2D components instead. They run on Box2D, the 2D physics engine behind games like Angry Birds and Limbo, and work the way Unity's 2D physics does. Everything moves across the XY plane and turns about Z, and each Obj keeps its own Z, so you can still layer sprites in front of each other.

Add each one with **Add Component**, under **Physics 2D**:

- A **Collider 2D** gives an Obj a flat shape: a box, a circle or a capsule.
- A **Rigidbody 2D** beside it hands the Obj over to 2D physics.

2D and 3D physics don't see each other. A Collider 2D never touches a Collider, so pick one kind for each game.

## Collider 2D

| Setting | What it does |
| --- | --- |
| Shape | Box, Circle or Capsule |
| Offset | Where the middle of the shape is, from the Obj's origin |
| Size | A box's width and height |
| Radius | A circle's or a capsule's radius |
| Height | A capsule's height, from end to end, standing up |
| Is trigger | Nothing bumps into it. It notices what passes through it instead, such as a coin being picked up |
| Friction | How much it grips what slides on it. 0 is ice |
| Bounciness | How much things bounce off it, from 0 to 1 |

The Scene view outlines the selected Obj's Collider 2D in green, flat against the XY plane.

## Rigidbody 2D

| Setting | What it does |
| --- | --- |
| Mass | How heavy it is, in kilograms |
| Is kinematic | Your code moves it rather than physics, and it pushes what is in its way: a moving platform |
| Gravity scale | How strongly gravity pulls it. 0 floats, 1 falls like everything else, 2 falls twice as hard |
| Drag | How quickly it slows down as it moves |
| Angular drag | How quickly it stops spinning |
| Fixed rotation | It never turns. Turn this on for a platformer's character, so it doesn't topple over |
| Continuous | Checks its whole path each step, so a fast bullet can't pass through a thin wall |

## From scripts

The 2D API mirrors the 3D one, with `2D` on the end of each name, as in Unity:

```csharp
public sealed class Jumper : Behaviour
{
	public float JumpSpeed = 6f;

	protected override void Update()
	{
		var body = Obj.Rigidbody2D;
		var feet = new Vector2(Obj.WorldPosition.X, Obj.WorldPosition.Y - 0.55f);
		var grounded = Physics2D.Raycast(feet, -Vector2.UnitY, out _, 0.1f);
		if (grounded && Input.Pressed("Jump"))
		{
			body.Velocity = new Vector2(body.Velocity.X, JumpSpeed);
		}
	}

	protected override void OnTriggerEnter2D(Collider2D other)
	{
		Log.Info($"Picked up {other.Obj.Name}");
	}
}
```

- `Obj.Collider2D` and `Obj.Rigidbody2D` reach the components, and `AddCollider2D` and `AddRigidbody2D` add them.
- `AddForce` and `AddTorque` take `ForceMode2D.Force` or `ForceMode2D.Impulse`. `AngularVelocity` is in degrees per second, as in Unity.
- `OnCollisionEnter2D`, `OnCollisionExit2D`, `OnTriggerEnter2D` and `OnTriggerExit2D` arrive straight after each physics step.
- `Physics2D.Raycast`, `Physics2D.Linecast` and `Physics2D.OverlapCircle` ask what is where, passing over triggers unless you pass `hitTriggers: true`, and take a layer mask as the 3D ones do. `Physics2D.Gravity` is which way everything falls.
- 2D bodies follow the same layers and Collision Matrix as 3D ones (see [[Colliders and Rigidbodies]]).

## In scene files

```toml
[[obj]]
id = 3
name = "Hero"
position = [0.0, 5.0, 0.0]
collider2d = { shape = "capsule", radius = 0.3, height = 1.2 }
rigidbody2d = { mass = 60.0, fixed_rotation = true }
```
