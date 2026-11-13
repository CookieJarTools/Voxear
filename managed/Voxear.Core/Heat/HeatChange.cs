namespace Voxear;

/// <summary>
/// One of a Heat Body's changes happening, as HeatBody.Changed tells of it: the Obj that changed, what it became
/// (None when the change becomes nothing), the change's name and its place in the Heat Body's list. The Obj is taken
/// away at the end of the frame when it became something.
/// </summary>
public readonly struct HeatChange
{
	internal HeatChange(Obj obj, Obj became, string name, int index)
	{
		Obj = obj;
		Became = became;
		Name = name;
		Index = index;
	}

	/// <summary>The Obj whose Heat Body changed.</summary>
	public Obj Obj { get; }

	/// <summary>The Obj it became, made from the change's Becomes prefab; None when it names none.</summary>
	public Obj Became { get; }

	/// <summary>The change's name, as the Inspector shows it: "Seared", "Frozen".</summary>
	public string Name { get; }

	/// <summary>Its place in the Heat Body's list of changes.</summary>
	public int Index { get; }

	public override string ToString()
	{
		return Became.IsValid ? $"{Obj} {Name}, now {Became}" : $"{Obj} {Name}";
	}
}
