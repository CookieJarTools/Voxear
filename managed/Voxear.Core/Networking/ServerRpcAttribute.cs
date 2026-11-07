using System;

namespace Voxear;

/// <summary>
/// Makes a behaviour's method run on the server when a player's machine asks. Write what the server does, and call
/// the method Voxear writes beside it, its name with OnServer after it:
/// <code>
/// public sealed partial class Gun : Behaviour
/// {
/// 	[ServerRpc]
/// 	private void Fire(Vector3 aim)
/// 	{
/// 		// Runs on the server, for the Obj's owner.
/// 	}
///
/// 	protected override void Update()
/// 	{
/// 		if (Input.Pressed("Fire"))
/// 		{
/// 			FireOnServer(Camera.Forward);
/// 		}
/// 	}
/// }
/// </code>
/// Only the Obj's owner may ask, unless AnyPlayer says otherwise, and Net.Caller says who did. The arguments can be
/// bool, int, long, float, double, string, Vector2, Vector3, Quaternion, enums, Obj and NetPlayer.
/// </summary>
[AttributeUsage(AttributeTargets.Method, Inherited = false)]
public sealed class ServerRpcAttribute : Attribute
{
	/// <summary>Lets any player ask, not only the Obj's owner, as for a door anyone may open.</summary>
	public bool AnyPlayer { get; set; }

	/// <summary>Lets it be lost, as for something sent again soon anyway: it goes sooner, never held up by others.</summary>
	public bool Unreliable { get; set; }

	/// <summary>How many times a second each player may ask; 0 leaves only the engine's own limit.</summary>
	public int PerSecond { get; set; }
}
