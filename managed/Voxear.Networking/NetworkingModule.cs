using System.Runtime.CompilerServices;
using Voxear.Scripting;

namespace Voxear;

/// <summary>Adds the Networking module to the engine's moments when its assembly is first read.</summary>
internal static class NetworkingModule
{
	// Voxear.Core reads a module's assembly itself and runs its initializer, which is what this rule's warning is for.
#pragma warning disable CA2255
	[ModuleInitializer]
	internal static void Start()
	{
		ModuleHooks.Dispatch += Net.Dispatch;
		ModuleHooks.TakeNetFields += Replication.TakeIn;
		ModuleHooks.Forget += Net.Forget;
	}
#pragma warning restore CA2255
}
