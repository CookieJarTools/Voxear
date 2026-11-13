namespace Voxear;

/// <summary>
/// Lets a player use the blocks of a Block World: on its Use action, the block straight ahead of its Eye within
/// Reach is used (see <see cref="Blocks.Use(System.Numerics.Vector3, Obj)"/>): a door opens, a lever turns over.
/// Put it on the player's camera, or on the player with its camera as its Eye. With no action picked it uses
/// Player/Interact.
/// </summary>
public sealed class BlockUser : Behaviour
{
	/// <summary>The action that uses a block.</summary>
	public InputActionRef Use;

	/// <summary>How far ahead a block is used, in metres.</summary>
	public float Reach = 4f;

	/// <summary>Where it looks from, and which way; its own Obj where none is given.</summary>
	public Obj Eye;

	protected override void Update()
	{
		var action = Use.IsSet ? Use : new InputActionRef("Player/Interact");
		if (!action.Action.Pressed)
		{
			return;
		}
		var eye = Eye.IsValid ? Eye : Obj;
		if (Blocks.Raycast(eye.WorldPosition, eye.Forward, out var hit, Reach))
		{
			Blocks.Use(hit, Obj);
		}
	}
}
