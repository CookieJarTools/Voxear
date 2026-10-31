---
id: c4b8e0d2a671
tags: [hello, scripts]
updated: 2026-10-31
---

Every behaviour in `Scripts/`, in a sentence each.

- **GameManager**: keeps score and moves through the levels. See [[The Day and Night Cycle]].
- **Spinner**: spins its Obj around the up axis, and checks in with the GameManager when it starts. Given a leader, it spins the other way at the leader's speed.
- **Turner**: turns its Obj, only around the vertical, until it faces a point.
- **Pulse**: breathes another Obj's scale in and out, such as a lamp's bulb.
- **FireflySpawner**: releases fireflies around lamps, and ~~waits for~~ uses common fireflies until the rare one has loaded.
- **Firefly**: drifts upward with a little wander, then destroys its own Obj.

```csharp
public sealed class Spinner : Behaviour
{
    [SerializeField] private float degreesPerSecond = 45f;
}
```

> [!NOTE]
> Behaviours do not run while editing; press **Play** to see them move.
