namespace Voxear;

/// <summary>One leg of a <see cref="ClipRig"/>: its bones from the thigh down, by their numbers; -1 for one it lacks.</summary>
public sealed class ClipLeg
{
	internal ClipLeg(RigReader read)
	{
		Thigh = read.Number();
		Shin = read.Number();
		Foot = read.Number();
		Toe = read.Number();
	}

	public int Thigh { get; }

	public int Shin { get; }

	public int Foot { get; }

	public int Toe { get; }
}
