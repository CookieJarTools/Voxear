using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>A sample that ends when disposed, from <see cref="Profiler.Sample"/> or <see cref="ProfilerMarker.Auto"/>.</summary>
public readonly struct ProfilerScope : IDisposable
{
	private readonly bool recorded;

	internal ProfilerScope(bool recorded)
	{
		this.recorded = recorded;
	}

	public void Dispose()
	{
		if (recorded)
		{
			Native.ProfilerEnd();
		}
	}
}
