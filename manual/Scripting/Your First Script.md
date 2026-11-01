---
order: 1
---

# Your First Script

This page makes a script that spins a cube.

## Making the script

1. In the Project panel, open the `Scripts` folder.
2. Right-click in it and choose **New C# Script**.
3. Type `Spinner` as its name and press Enter.
4. Double-click it to open it in your code editor.

Keep your scripts in the `Scripts` folder, or in folders inside it. Scripts anywhere else are not built.

The new script looks like this, with your project's name in place of `MyGame`:

```csharp
using Voxear;

namespace MyGame
{
	public sealed class Spinner : Behaviour
	{
		protected override void Start()
		{
		}

		protected override void Update()
		{
		}
	}
}
```

## Making it spin

Change it to this, and save:

```csharp
using System.Numerics;
using Voxear;

namespace MyGame
{
	/// <summary>Spins its Obj around the up axis.</summary>
	public sealed class Spinner : Behaviour
	{
		[SerializeField] private float degreesPerSecond = 90f;

		protected override void Update()
		{
			var turn = Quaternion.AngleAxis(degreesPerSecond * Time.Delta, Vector3.Up);
			Obj.Rotation = Quaternion.Normalize(Obj.Rotation * turn);
		}
	}
}
```

- `Update` runs once every frame. A frame is one picture of the game, and the game draws many each second.
- `Obj` is the Obj this behaviour is on.
- `Time.Delta` is how many seconds the last frame took, so the cube turns at the same speed however fast the computer is.
- `degreesPerSecond` shows in the Inspector, so you can change the speed without changing the code. See [[Fields in the Inspector]].

When you save, the editor builds your scripts in the background. If something is wrong, the Console lists the errors under **Compiler**.

## Putting it on a cube

1. Make a cube: the **+** at the top of the Hierarchy, then **Cube**.
2. With the cube selected, click **Add Component** in the Inspector and pick **Spinner**.
3. Click **Play**.

The cube spins. Change **Degrees Per Second** in the Inspector while it plays to try other speeds. Remember that changes made while playing go away when you stop; see [[Playing Your Game]].

## Writing to the Console

```csharp
Log.Info("The cube is spinning");
Log.Warning("Spinning very fast");
Log.Error("The cube fell off the world");
```

Each message shows in the Console. Double-click it to open the line of code that wrote it.

## Things to watch for

- Write lifecycle methods as `protected override void Update()`. A plain `void Update()`, as in Unity, never runs. The compiler warns about it.
- Write your `using` lines yourself: `using System.Numerics;` for vectors and rotations, and `using System;` for `Random`.
- Logging is `Log.Info`, not `Debug.Log`. In Voxear, `Debug` draws lines and shapes in the Scene view.

Next: [[How Scripts Run]].
