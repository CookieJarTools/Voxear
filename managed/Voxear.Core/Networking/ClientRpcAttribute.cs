using System;

namespace Voxear;

/// <summary>
/// Makes a behaviour's method run on players' machines when the server asks, as for an explosion everyone sees.
/// Write what each machine does, and call the methods Voxear writes beside it, on the server: its name with
/// OnClients after it for every player who has the Obj, or OnClient for one.
/// <code>
/// [ClientRpc]
/// private void Exploded(Vector3 at)
/// {
/// 	// Runs on every player's machine, a host's too.
/// }
///
/// // On the server:
/// ExplodedOnClients(Obj.Position);
/// </code>
/// </summary>
[AttributeUsage(AttributeTargets.Method, Inherited = false)]
public sealed class ClientRpcAttribute : Attribute
{
	/// <summary>Lets it be lost, as for an effect that matters only now: it goes sooner, never held up by others.</summary>
	public bool Unreliable { get; set; }
}
