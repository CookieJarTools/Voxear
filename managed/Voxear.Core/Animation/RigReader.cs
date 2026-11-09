using System;
using System.Collections.Generic;

namespace Voxear;

/// <summary>Reads a rig's bones in the order the engine lays them out.</summary>
internal sealed class RigReader
{
	private readonly Skeleton skeleton;
	private readonly float[] layout;
	private int at;

	public RigReader(Skeleton skeleton, ReadOnlySpan<float> layout)
	{
		this.skeleton = skeleton;
		this.layout = layout.ToArray();
	}

	public int Number()
	{
		return at < layout.Length ? (int)layout[at++] : -1;
	}

	public Bone Bone()
	{
		var index = Number();
		return index >= 0 ? skeleton[index] : Voxear.Bone.None;
	}

	/// <summary>A list of bones as their numbers, for a rig of a skeleton that no Obj draws.</summary>
	public IReadOnlyList<int> Numbers()
	{
		var count = Math.Max(Number(), 0);
		var numbers = new int[count];
		for (var i = 0; i < count; i++)
		{
			numbers[i] = Number();
		}
		return numbers;
	}

	public IReadOnlyList<Bone> Bones()
	{
		var count = Math.Max(Number(), 0);
		var bones = new Bone[count];
		for (var i = 0; i < count; i++)
		{
			bones[i] = Bone();
		}
		return bones;
	}
}
