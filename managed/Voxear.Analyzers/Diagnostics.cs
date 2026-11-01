using Microsoft.CodeAnalysis;

namespace Voxear.Analyzers
{
	/// <summary>What the analyzer reports, each with a number of its own so a project can turn one down.</summary>
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

		private static DiagnosticDescriptor Error(string id, string title, string message)
		{
			return new DiagnosticDescriptor(id, title, message, Category, DiagnosticSeverity.Error, isEnabledByDefault: true);
		}
	}
}
