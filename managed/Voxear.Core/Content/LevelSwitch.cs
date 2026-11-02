namespace Voxear;

/// <summary>A level giving way to another: what is being left, what is coming, and the change itself.</summary>
public readonly struct LevelSwitch
{
	public LevelSwitch(string? leaving, LevelLoad load)
	{
		Leaving = leaving;
		Load = load;
	}

	/// <summary>The level that was loaded, or null at the first change of the game.</summary>
	public string? Leaving { get; }

	public string Entering => Load.Level;

	public LevelLoad Load { get; }
}
