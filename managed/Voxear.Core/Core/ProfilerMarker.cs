using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A named sample for the editor's Profiler, made once and kept, as Unity's ProfilerMarker:
/// <c>static readonly ProfilerMarker Pathfinding = new ProfilerMarker("Pathfinding");</c>, then
/// <c>using (Pathfinding.Auto()) { ... }</c>.
/// </summary>
public readonly struct ProfilerMarker
{
	private readonly uint id;

	public ProfilerMarker(string name)
	{
		Name = name;
		id = Native.ProfilerMarker(name);
	}

	public string Name { get; }

	/// <summary>
	/// Starts a sample, returning whether it was recorded, as it is not while the Profiler is not taking samples.
	/// Only a sample that was recorded is ended, with <see cref="End"/>.
	/// </summary>
	public bool Begin()
	{
		return Native.ProfilerBegin(id);
	}

	public void End()
	{
		Native.ProfilerEnd();
	}

	/// <summary>A sample that ends when the scope it returns is disposed, at the end of a using block.</summary>
	public ProfilerScope Auto()
	{
		return new ProfilerScope(Begin());
	}
}
