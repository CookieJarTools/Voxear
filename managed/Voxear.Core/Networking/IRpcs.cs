namespace Voxear;

/// <summary>
/// A behaviour with RPCs, which Voxear writes for it: running one that came, by its key. Game code does not
/// implement this itself.
/// </summary>
public interface IRpcs
{
	/// <summary>Runs the RPC with this key, if it is one of this behaviour's, reading its arguments; false if not.</summary>
	bool ReceiveRpc(uint key, NetReader reader);
}
