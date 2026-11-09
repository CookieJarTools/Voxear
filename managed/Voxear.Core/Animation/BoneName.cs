namespace Voxear;

/// <summary>
/// A bone named before the model it is in was there to find it in, as a scene's field names one: found by its name
/// the first time the model has it, and kept.
/// </summary>
internal sealed class BoneName
{
	private int found = -1;

	public BoneName(string name)
	{
		Name = name;
	}

	public string Name { get; }

	/// <summary>The bone's number in the model the Obj draws, or -1 while it has no bone of the name.</summary>
	public int IndexIn(Obj obj)
	{
		if (found < 0)
		{
			var bone = new Skeleton(obj).Find(Name);
			found = bone.IsValid ? bone.Index : -1;
		}
		return found;
	}
}
