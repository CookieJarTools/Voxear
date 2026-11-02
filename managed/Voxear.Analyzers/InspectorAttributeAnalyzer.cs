using System.Collections.Immutable;
using System.Linq;
using System.Text.RegularExpressions;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.Diagnostics;

namespace Voxear.Analyzers;

/// <summary>
/// Checks the Inspector's attributes where they are written. Conditions and callbacks usually name members with
/// nameof, which the compiler checks already; this catches the rest: a name in quotes that is wrong, a member of
/// the wrong kind or shape, and values an attribute cannot use.
/// </summary>
[DiagnosticAnalyzer(LanguageNames.CSharp)]
public sealed class InspectorAttributeAnalyzer : DiagnosticAnalyzer
{
	private static readonly string[] Conditions = { "ShowIfAttribute", "HideIfAttribute", "EnableIfAttribute", "DisableIfAttribute" };

	private static readonly string[] NumberAttributes = { "RangeAttribute", "MinValueAttribute", "MaxValueAttribute", "ProgressBarAttribute" };

	private static readonly Regex Hex = new Regex("^#?[0-9a-fA-F]{6}$");

	public override ImmutableArray<DiagnosticDescriptor> SupportedDiagnostics => ImmutableArray.Create(
		Diagnostics.MissingField, Diagnostics.BadCallback, Diagnostics.BadValidator, Diagnostics.BadDropdown,
		Diagnostics.BadButton, Diagnostics.NotANumber, Diagnostics.BadColour);

	public override void Initialize(AnalysisContext context)
	{
		context.ConfigureGeneratedCodeAnalysis(GeneratedCodeAnalysisFlags.None);
		context.EnableConcurrentExecution();
		context.RegisterSymbolAction(AnalyzeField, SymbolKind.Field);
		context.RegisterSymbolAction(AnalyzeMethod, SymbolKind.Method);
	}

	private static void AnalyzeMethod(SymbolAnalysisContext context)
	{
		var method = (IMethodSymbol)context.Symbol;
		foreach (var attribute in VoxearAttributes(method))
		{
			if (attribute.AttributeClass!.Name == "ButtonAttribute" && method.Parameters.Length > 0)
			{
				Report(context, Diagnostics.BadButton, attribute, method.Name);
			}
		}
	}

	private static void AnalyzeField(SymbolAnalysisContext context)
	{
		var field = (IFieldSymbol)context.Symbol;
		foreach (var attribute in VoxearAttributes(field))
		{
			Check(context, field, attribute);
		}
	}

	private static void Check(SymbolAnalysisContext context, IFieldSymbol field, AttributeData attribute)
	{
		var name = attribute.AttributeClass!.Name;
		var type = field.ContainingType;
		switch (name)
		{
			case "OnValueChangedAttribute":
				CheckCallback(context, attribute, type, Text(attribute, 0));
				break;
			case "ValidateInputAttribute":
				CheckValidator(context, attribute, field, Text(attribute, 0));
				break;
			case "ValueDropdownAttribute":
				CheckDropdown(context, attribute, type, Text(attribute, 0));
				break;
			case "ColorPaletteAttribute":
				CheckColours(context, attribute);
				break;
			case "InfoBoxAttribute":
				CheckCondition(context, attribute, type, Text(attribute, 2));
				break;
			default:
				CheckConditionOrNumber(context, attribute, field);
				break;
		}
	}

	private static void CheckConditionOrNumber(SymbolAnalysisContext context, AttributeData attribute, IFieldSymbol field)
	{
		var name = attribute.AttributeClass!.Name;
		if (Conditions.Contains(name))
		{
			CheckCondition(context, attribute, field.ContainingType, Text(attribute, 0));
		}
		else if (NumberAttributes.Contains(name) && !Members.IsNumber(field.Type))
		{
			Report(context, Diagnostics.NotANumber, attribute, name.Substring(0, name.Length - "Attribute".Length), field.Name);
		}
	}

	/// <summary>A condition reads another field of the same type, as the scene sets it.</summary>
	private static void CheckCondition(SymbolAnalysisContext context, AttributeData attribute, ITypeSymbol type, string? member)
	{
		if (member != null && !Members.HasField(type, member))
		{
			var shown = $"[{attribute.AttributeClass!.Name.Replace("Attribute", string.Empty)}]";
			Report(context, Diagnostics.MissingField, attribute, shown, member, type.Name);
		}
	}

	private static void CheckCallback(SymbolAnalysisContext context, AttributeData attribute, ITypeSymbol type, string? method)
	{
		var found = method != null && Members.Named(type, method).OfType<IMethodSymbol>().Any(m => m.Parameters.Length == 0);
		if (!found)
		{
			Report(context, Diagnostics.BadCallback, attribute, method ?? "nothing", type.Name);
		}
	}

	/// <summary>A check takes the field's value and answers bool, or string, empty when the value is right.</summary>
	private static void CheckValidator(SymbolAnalysisContext context, AttributeData attribute, IFieldSymbol field, string? method)
	{
		var methods = method == null ? Enumerable.Empty<IMethodSymbol>() : Members.Named(field.ContainingType, method).OfType<IMethodSymbol>();
		var found = methods.Any(m => m.Parameters.Length == 1
			&& SymbolEqualityComparer.Default.Equals(m.Parameters[0].Type, field.Type)
			&& (m.ReturnType.SpecialType == SpecialType.System_Boolean || m.ReturnType.SpecialType == SpecialType.System_String));
		if (!found)
		{
			Report(context, Diagnostics.BadValidator, attribute, method ?? "nothing", field.ContainingType.Name, field.Type.Name);
		}
	}

	/// <summary>A dropdown's values come from a method that takes nothing, a property or a field, holding a list.</summary>
	private static void CheckDropdown(SymbolAnalysisContext context, AttributeData attribute, ITypeSymbol type, string? member)
	{
		var members = member == null ? Enumerable.Empty<ISymbol>() : Members.Named(type, member);
		if (!members.Any(GivesList))
		{
			Report(context, Diagnostics.BadDropdown, attribute, member ?? "nothing", type.Name);
		}
	}

	private static bool GivesList(ISymbol member)
	{
		switch (member)
		{
			case IMethodSymbol method:
				return method.Parameters.Length == 0 && Members.IsList(method.ReturnType);
			case IPropertySymbol property:
				return Members.IsList(property.Type);
			case IFieldSymbol field:
				return Members.IsList(field.Type);
			default:
				return false;
		}
	}

	private static void CheckColours(SymbolAnalysisContext context, AttributeData attribute)
	{
		if (attribute.ConstructorArguments.Length == 0 || attribute.ConstructorArguments[0].Kind != TypedConstantKind.Array)
		{
			return;
		}
		foreach (var colour in attribute.ConstructorArguments[0].Values.Select(v => v.Value as string))
		{
			if (colour == null || !Hex.IsMatch(colour))
			{
				Report(context, Diagnostics.BadColour, attribute, colour ?? "null");
			}
		}
	}

	private static ImmutableArray<AttributeData> VoxearAttributes(ISymbol symbol)
	{
		return symbol.GetAttributes().Where(a => a.AttributeClass?.ContainingNamespace?.ToDisplayString() == "Voxear").ToImmutableArray();
	}

	/// <summary>The constructor's argument at index, when it is text.</summary>
	private static string? Text(AttributeData attribute, int index)
	{
		return index < attribute.ConstructorArguments.Length ? attribute.ConstructorArguments[index].Value as string : null;
	}

	private static void Report(SymbolAnalysisContext context, DiagnosticDescriptor descriptor, AttributeData attribute, params object[] words)
	{
		var location = attribute.ApplicationSyntaxReference?.GetSyntax(context.CancellationToken).GetLocation()
			?? context.Symbol.Locations.FirstOrDefault();
		context.ReportDiagnostic(Diagnostic.Create(descriptor, location, words));
	}
}
