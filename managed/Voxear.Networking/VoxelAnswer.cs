namespace Voxear;

/// <summary>What <see cref="VoxelRequests.Check"/> says of a request: made, or refused and why.</summary>
public readonly record struct VoxelAnswer(bool Accepted, string Reason)
{
	public static VoxelAnswer Accept => new(true, "");

	public static VoxelAnswer Refuse(string reason)
	{
		return new(false, reason);
	}
}
