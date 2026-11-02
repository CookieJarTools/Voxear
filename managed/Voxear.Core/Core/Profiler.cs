using System.Collections.Generic;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// Samples of a frame's work for the editor's Profiler, which shows them in its Hierarchy with their total and
/// self time and how many times they ran, as Unity's does. Each behaviour's Update and other callbacks are sampled
/// already, under their class and method; these are for the parts of one worth seeing on their own. The easy form is
/// a block: <c>using (Profiler.Sample("Pathfinding")) { ... }</c>. For a sample taken often, a
/// <see cref="ProfilerMarker"/> kept in a static field skips looking its name up.
/// </summary>
public static class Profiler
{
	private static readonly Dictionary<string, ProfilerMarker> markers = new Dictionary<string, ProfilerMarker>();
	private static readonly Stack<bool> begun = new Stack<bool>();

	/// <summary>Whether the editor's Profiler is taking samples now. Nothing is recorded while it is not.</summary>
	public static bool Enabled => Native.ProfilingEnabled;

	/// <summary>A sample of the code in a using block.</summary>
	public static ProfilerScope Sample(string name)
	{
		return Marker(name).Auto();
	}

	/// <summary>Starts a sample that <see cref="EndSample"/> ends, as Unity's Profiler.BeginSample.</summary>
	public static void BeginSample(string name)
	{
		begun.Push(Marker(name).Begin());
	}

	public static void EndSample()
	{
		if (begun.TryPop(out var recorded) && recorded)
		{
			Native.ProfilerEnd();
		}
	}

	private static ProfilerMarker Marker(string name)
	{
		if (!markers.TryGetValue(name, out var marker))
		{
			marker = new ProfilerMarker(name);
			markers[name] = marker;
		}
		return marker;
	}
}
