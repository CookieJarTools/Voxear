using Voxear;

namespace {{Namespace}};

/// <summary>
/// Lets the player use the blocks of a Block World, as the engine's Block User does: on its Use action, the block
/// straight ahead of its Eye within Reach is used, so a door opens and a lever turns over. With no action picked it
/// uses Player/Interact; with no Eye it looks from its own Obj.
/// </summary>
public sealed class UsesBlocks : Behaviour
{
	[SerializeField, Tooltip("The action that uses a block")] private InputActionRef use;
	[SerializeField, Suffix("m"), Tooltip("How far ahead of the Eye a block is used")] private float reach = 10f;
	[SerializeField, Tooltip("Where it looks from, and which way: the player's camera")] private Obj eye;

	protected override void Update()
	{
		var action = use.IsSet ? use : new InputActionRef("Player/Interact");
		if (!action.Action.Pressed)
		{
			return;
		}
		var from = eye.IsValid ? eye : Obj;
		if (Blocks.Raycast(from.WorldPosition, from.Forward, out var hit, reach))
		{
			Blocks.Use(hit, Obj);
		}
	}
}
