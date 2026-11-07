using System;
using System.Runtime.Loader;

namespace Voxear.Scripting;

/// <summary>
/// What an optional module's assembly does at the engine's own moments. Voxear.Core cannot name a module's types, since
/// a project without the module has no such assembly, so each module adds itself here when it loads.
/// </summary>
internal static class ModuleHooks
{
	/// <summary>Each frame, before the game's Update: a module's own events, such as players joining.</summary>
	public static Action? Dispatch;

	/// <summary>A predicted Obj put right by the server: the replicated properties that came with it.</summary>
	public static Action? TakeNetFields;

	/// <summary>The game's scripts reloaded: whatever a module holds of the old ones lets go.</summary>
	public static Action<AssemblyLoadContext>? Forget;
}
