namespace Voxear;

/// <summary>
/// A behaviour with replicated properties, which Voxear writes for it: how its values go into bytes for the network
/// and come back out. Game code does not implement this itself.
/// </summary>
public interface IReplicated
{
	void WriteReplicated(NetWriter writer);

	void ReadReplicated(NetReader reader);
}
