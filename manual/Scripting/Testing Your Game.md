---
order: 21
---

A **test** is a static method that checks one thing about your game and says whether it holds: that a door opens for its key, that a save loads back what was saved, that a tool you wrote for building levels leaves them as it should. Write them as you go, and run them all before you share a build, so what used to work still does.

```csharp
using System.Numerics;
using Voxear;
using Voxear.Testing;

namespace MyGame;

internal static class GameTests
{
	[Test("The vault has its door", Category = "Levels")]
	private static void VaultHasDoor()
	{
		var door = Obj.Find("Vault Door");
		Assert.True(door.IsValid, "the level has its vault door");
		Assert.Near(new Vector3(0f, 0f, 12f), door.Position, 0.01f);
	}

	[Test("A hit never heals", Category = "Combat")]
	private static void HitNeverHeals()
	{
		Assert.Equal(70, Health.AfterHit(100, 30));
		Assert.Equal(100, Health.AfterHit(100, -5), "a negative hit does nothing");
	}
}
```
- The method must be `static`, take nothing, and return nothing. `[Test]` without a title uses the method's name.
- It passes when it returns. It fails when an `Assert` does not hold, with what the Assert expected and what it found, or when it throws, with what it threw and where.
- While you edit, a test runs against the level you are editing, and Undo takes back anything it changed in one step. Your behaviours do not run while you edit, so a test finds Objs but not their behaviours; press Play first to test them, and the test runs against the running game.

## The Tests window

Open it from the top bar's **Window** button, or type *Tests* in the command bar. It lists every test, grouped by **Category**: those with none are under *General*.

- **Run All** runs every test, the play button on a category's heading runs that category's, and the one on a test's row runs just that test.
- Each test shows a mark for how it came out (green passed, red failed, pulsing while it waits), how long it took, and under it what a failure said.
- The filter narrows the list by a test's title or category.

Tests run one after another in a set order: categories in the order of their lowest `Order`, then by name, and in a category by `Order`, then by title. Give a test that sets something up for the next a lower `Order`.

## What Assert checks

| Assert | Fails unless |
| --- | --- |
| `Assert.True(condition)`, `Assert.False(condition)` | the condition holds, or does not |
| `Assert.Equal(expected, actual)`, `Assert.NotEqual(unexpected, actual)` | the two are equal, or not |
| `Assert.Near(expected, actual, tolerance)` | two numbers or two `Vector3`s are within the tolerance |
| `Assert.Null(value)`, `Assert.NotNull(value)` | the value is null, or is not |
| `Assert.Throws<T>(() => ...)` | the code throws an exception of type `T`, which it gives back |
| `Assert.Fail(message)` | never: it fails with your message |

Each takes a message last, which comes first in what a failure says.

## Running tests from a terminal

`vox editor call tests.run` runs them in the open editor, all of them or `{"category": "Doors"}` or `{"test": "OpensForKey"}`, and `vox editor call tests.list` says how each came out. An AI assistant connected with `vox mcp` can run your tests the same way. See [[Driving the Editor from a Terminal]].
