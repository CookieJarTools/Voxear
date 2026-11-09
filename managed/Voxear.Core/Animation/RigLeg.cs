namespace Voxear;

/// <summary>One leg of a <see cref="Rig"/>: its bones from the thigh down.</summary>
public sealed class RigLeg
{
	internal RigLeg(RigReader read)
	{
		Thigh = read.Bone();
		Shin = read.Bone();
		Foot = read.Bone();
		Toe = read.Bone();
	}

	public Bone Thigh { get; }

	public Bone Shin { get; }

	public Bone Foot { get; }

	/// <summary>The toes, or <see cref="Bone.None"/> for a rig without them.</summary>
	public Bone Toe { get; }
}
