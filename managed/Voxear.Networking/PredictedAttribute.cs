using System;

namespace Voxear;

/// <summary>
/// Runs a behaviour's FixedUpdate on its Obj's owner's machine as well as the server, with the owner's input, so
/// what they press moves their own Obj at once rather than a round trip later:
/// <code>
/// [Predicted]
/// public sealed partial class Walker : Behaviour
/// {
/// 	[Replicated]
/// 	public partial float Fall { get; set; }
///
/// 	protected override void FixedUpdate()
/// 	{
/// 		if (Obj.NetObject.Owner is not NetPlayer player)
/// 		{
/// 			return;
/// 		}
/// 		var move = player.Vector2("Move");
/// 		Obj.CharacterController.Move(new Vector3(move.X, Fall, move.Y) * Time.Delta);
/// 	}
/// }
/// </code>
/// When the server's state disagrees with the owner's, the Obj is put back as the server had it, with its replicated
/// properties, and the ticks since run again. Keep everything FixedUpdate changes from tick to tick in replicated
/// properties, and move with a Character Controller, which moves at once, rather than a Rigidbody. Net.IsReplaying
/// says when ticks are run again, for effects that should play only once; RPCs made then are not sent.
/// </summary>
[AttributeUsage(AttributeTargets.Class, Inherited = false)]
public sealed class PredictedAttribute : Attribute
{
}
