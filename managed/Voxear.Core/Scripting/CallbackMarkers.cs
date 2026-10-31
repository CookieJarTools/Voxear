using System.Collections.Generic;
using Voxear.Interop;

namespace Voxear.Scripting
{
	/// <summary>
	/// The Profiler's marker for each behaviour callback, as "Hello.Spinner.Update()", made the first time it runs.
	/// Kept by class name rather than by type, so a hot reload's old classes are not held in memory.
	/// </summary>
	internal static class CallbackMarkers
	{
		private const int Phases = (int)Phase.OnDestroy + 1;

		private static readonly Dictionary<string, uint[]> markers = new Dictionary<string, uint[]>();

		public static uint For(Behaviour behaviour, Phase phase)
		{
			var name = behaviour.GetType().FullName!;
			if (!markers.TryGetValue(name, out var byPhase))
			{
				byPhase = new uint[Phases];
				markers[name] = byPhase;
			}

			// Marker ids start at zero, so one is stored plus one, leaving zero for not made yet.
			if (byPhase[(int)phase] == 0)
			{
				byPhase[(int)phase] = Native.ProfilerMarker($"{name}.{phase}()") + 1;
			}
			return byPhase[(int)phase] - 1;
		}
	}
}
