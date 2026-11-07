using System.Collections.Generic;

namespace Voxear.Analyzers;

/// <summary>The names each of the engine's optional modules gives scripts, for saying which module a missing one is in.</summary>
internal static class ModuleNames
{
	private static readonly Dictionary<string, string> modules = new Dictionary<string, string>
	{
		["Net"] = "Networking",
		["NetObject"] = "Networking",
		["AddNetObject"] = "Networking",
		["NetPlayer"] = "Networking",
		["NetRole"] = "Networking",
		["NetAuthority"] = "Networking",
		["NetReader"] = "Networking",
		["NetWriter"] = "Networking",
		["Rpc"] = "Networking",
		["IReplicated"] = "Networking",
		["IRpcs"] = "Networking",
		["Replicated"] = "Networking",
		["ReplicatedAttribute"] = "Networking",
		["ServerRpc"] = "Networking",
		["ServerRpcAttribute"] = "Networking",
		["ClientRpc"] = "Networking",
		["ClientRpcAttribute"] = "Networking",
		["Predicted"] = "Networking",
		["PredictedAttribute"] = "Networking",
		["Gun"] = "Guns",
		["AddGun"] = "Guns",
		["GunHolder"] = "Guns",
		["AddGunHolder"] = "Guns",
		["GunKind"] = "Guns",
		["Shot"] = "Guns",
		["GunAction"] = "Guns",
		["AmmoKnowledge"] = "Guns",
		["AmmoEstimate"] = "Guns",
	};

	/// <summary>The module a name is part of, or null for a name no module gives.</summary>
	public static string? Of(string name)
	{
		return modules.TryGetValue(name, out var module) ? module : null;
	}
}
