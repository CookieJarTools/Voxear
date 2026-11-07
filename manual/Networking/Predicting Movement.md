---
order: 4
---

A player's input reaches the server a moment after they press, and the server's answer takes as long again to come back. Waiting for it would make their own character feel slow to answer. Instead, their machine **predicts**: it moves their own Objs at once, as the server will, and puts them right on the rare occasions the server disagrees.

## Predicted behaviours

Mark a behaviour `[Predicted]`. Its FixedUpdate then runs on the owner's machine as well as on the server, each tick, with the owner's input:

```csharp
[Predicted]
public sealed partial class Walker : Behaviour
{
	[Replicated]
	public partial float Fall { get; set; }

	protected override void FixedUpdate()
	{
		if (Obj.NetObject.Owner is not NetPlayer player)
		{
			return;
		}
		var move = player.Vector2("Move");
		Fall = Obj.CharacterController.IsGrounded ? 0f : Fall - 9.8f * Time.Delta;
		Obj.CharacterController.Move(new Vector3(move.X * 5f, Fall, move.Y * 5f) * Time.Delta);
	}
}
```

Other players' machines leave it alone, and show the Obj as the server moves it.

## When the server disagrees

The owner's machine keeps where the Obj was after each tick. When the server's state for a tick arrives, they are compared. If they are more than a centimetre apart, the Obj is put back as the server had it on that tick, its replicated properties too, and the ticks since then run again with the input they had. The difference is shown going away over a tenth of a second, so a small correction is never a jump. One of more than two metres, as a teleport, is shown at once.

For this to work, keep it simple:

- **Keep what changes in replicated properties.** Anything FixedUpdate carries from one tick to the next, as a fall speed or a dash's time left, should be `[Replicated]`, so it can be put back too.
- **Move with a Character Controller.** It moves at once, so a tick can run again on its own. A Rigidbody moves only when physics steps.
- **Play effects once.** `Net.IsReplaying` is true while ticks run again: play sounds and particles only when it is false. RPCs made while replaying are not sent, as they were the first time.

A player's own Objs go first in every update the server sends them, so their machine checks its predictions every tick. The F1 stats count the corrections on a client.
