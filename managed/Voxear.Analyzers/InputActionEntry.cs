namespace Voxear.Analyzers;

/// <summary>An input action as Actions names it: its map, its name, and the id a field or code knows it by.</summary>
internal readonly struct InputActionEntry
{
	public InputActionEntry(string map, string name, string id)
	{
		Map = map;
		Name = name;
		Id = id;
	}

	public string Map { get; }

	public string Name { get; }

	public string Id { get; }
}
