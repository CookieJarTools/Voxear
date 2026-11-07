using Microsoft.CodeAnalysis;

namespace Voxear.Analyzers;

/// <summary>What the analyzer and the Strings generator report, each with a number of its own so a project can turn one down.</summary>
internal static class Diagnostics
{
	private const string Category = "Voxear.Inspector";

	public static readonly DiagnosticDescriptor MissingField = Error(
		"VOX0001", "A condition names no field", "{0} names {1}, which is not a field of {2}");

	public static readonly DiagnosticDescriptor BadCallback = Error(
		"VOX0002", "[OnValueChanged] names no method that takes nothing", "[OnValueChanged] names {0}, which is not a method of {1} that takes nothing");

	public static readonly DiagnosticDescriptor BadValidator = Error(
		"VOX0003", "[ValidateInput] names no method that checks the value",
		"[ValidateInput] names {0}, which is not a method of {1} that takes a {2} and returns bool or string");

	public static readonly DiagnosticDescriptor BadDropdown = Error(
		"VOX0004", "[ValueDropdown] names nothing that gives values",
		"[ValueDropdown] names {0}, which is not a method taking nothing, a property or a field of {1} that gives a list");

	public static readonly DiagnosticDescriptor BadButton = Error(
		"VOX0005", "[Button] on a method that takes parameters", "[Button] is on {0}, which takes parameters; a button calls a method that takes nothing");

	public static readonly DiagnosticDescriptor NotANumber = new DiagnosticDescriptor(
		"VOX0006", "A number's attribute on a field that is not a number", "[{0}] is on {1}, which is not a number",
		Category, DiagnosticSeverity.Warning, isEnabledByDefault: true);

	public static readonly DiagnosticDescriptor BadColour = Error(
		"VOX0007", "[ColorPalette] has something that is not a colour", "\"{0}\" is not a colour; write it as #rrggbb");

	public static readonly DiagnosticDescriptor LeftOutKey = new DiagnosticDescriptor(
		"VOX0008", "A string table's key is left out of Strings", "\"{0}\" is left out of Strings, {1}",
		"Voxear.Localization", DiagnosticSeverity.Warning, isEnabledByDefault: true);

	public static readonly DiagnosticDescriptor NotPartial = new DiagnosticDescriptor(
		"VOX0009", "[Replicated] on a property Voxear cannot write",
		"[Replicated] is on {0} of {1}; make it a partial property, of a partial class that is neither nested nor generic",
		"Voxear.Networking", DiagnosticSeverity.Error, isEnabledByDefault: true);

	public static readonly DiagnosticDescriptor NotReplicable = new DiagnosticDescriptor(
		"VOX0010", "[Replicated] on a property of a type that cannot be replicated",
		"[Replicated] is on {0}, a {1}; bool, int, long, float, double, string, Vector2, Vector3, Quaternion, enums, Obj and NetPlayer can be replicated",
		"Voxear.Networking", DiagnosticSeverity.Error, isEnabledByDefault: true);

	public static readonly DiagnosticDescriptor BadRpc = new DiagnosticDescriptor(
		"VOX0011", "An RPC on a method Voxear cannot write a sender for",
		"[{0}] is on {1} of {2}; make it a method that returns void, with no type or ref parameters, of a partial class that is neither nested nor generic",
		"Voxear.Networking", DiagnosticSeverity.Error, isEnabledByDefault: true);

	public static readonly DiagnosticDescriptor NotSendable = new DiagnosticDescriptor(
		"VOX0012", "An RPC takes something that cannot be sent",
		"{0} of {1} is a {2}; bool, int, long, float, double, string, Vector2, Vector3, Quaternion, enums, Obj and NetPlayer can be sent",
		"Voxear.Networking", DiagnosticSeverity.Error, isEnabledByDefault: true);

	public static readonly DiagnosticDescriptor SameRpcName = new DiagnosticDescriptor(
		"VOX0013", "Two RPCs with one name",
		"{1} has more than one RPC named {0}; RPCs are told apart by name, so give each its own",
		"Voxear.Networking", DiagnosticSeverity.Error, isEnabledByDefault: true);

	public static readonly DiagnosticDescriptor ModuleOff = new DiagnosticDescriptor(
		"VOX0014", "A module's API in a project without the module",
		"{0} is part of the {1} module, which this project does not have; switch it on in Project Settings",
		"Voxear.Modules", DiagnosticSeverity.Error, isEnabledByDefault: true);

	private static DiagnosticDescriptor Error(string id, string title, string message)
	{
		return new DiagnosticDescriptor(id, title, message, Category, DiagnosticSeverity.Error, isEnabledByDefault: true);
	}
}
