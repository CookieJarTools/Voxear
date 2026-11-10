using System.Linq;
using Microsoft.CodeAnalysis;

namespace Voxear.Analyzers;

/// <summary>The types a NetWriter writes and a NetReader reads, for replicated properties and RPCs' arguments.</summary>
internal static class NetTypes
{
	/// <summary>
	/// The kind of a Vector3 marked [WorldPosition]: sent measured from the level's middle (see Voxear.Origin), so
	/// players whose worlds' middles have moved apart agree on where it is.
	/// </summary>
	public const string WorldPosition = "ReadLevelPosition";

	/// <summary>
	/// How the value of a property or parameter is read: a world position as one, anything else as
	/// <see cref="Kind(ITypeSymbol)"/> says.
	/// </summary>
	public static string? Kind(ISymbol marked, ITypeSymbol type)
	{
		return IsWorldPosition(marked, type) ? WorldPosition : Kind(type);
	}

	/// <summary>Whether a Vector3 is marked [WorldPosition].</summary>
	public static bool IsWorldPosition(ISymbol marked, ITypeSymbol type)
	{
		var vector = type.ToDisplayString() == "System.Numerics.Vector3";
		var attributes = marked.GetAttributes();
		return vector && attributes.Any(a => a.AttributeClass?.ToDisplayString() == "Voxear.WorldPositionAttribute");
	}

	/// <summary>How a value of this type is read, by the NetReader method, "enum" for an enum, or null for one that cannot be sent.</summary>
	public static string? Kind(ITypeSymbol type)
	{
		if (type.TypeKind == TypeKind.Enum)
		{
			return "enum";
		}
		return type.SpecialType switch
		{
			SpecialType.System_Boolean => "ReadBool",
			SpecialType.System_Int32 => "ReadInt",
			SpecialType.System_Int64 => "ReadLong",
			SpecialType.System_Single => "ReadFloat",
			SpecialType.System_Double => "ReadDouble",
			SpecialType.System_String => "ReadString",
			_ => type.ToDisplayString() switch
			{
				"System.Numerics.Vector2" => "ReadVector2",
				"System.Numerics.Vector3" => "ReadVector3",
				"System.Numerics.Quaternion" => "ReadQuaternion",
				"Voxear.Obj" => "ReadObj",
				"Voxear.NetPlayer" => "ReadPlayer",
				_ => null,
			},
		};
	}

	/// <summary>
	/// How an RPC's argument is read: as <see cref="Kind(ISymbol, ITypeSymbol)"/> says, and bytes too, which change
	/// too much to replicate.
	/// </summary>
	public static string? RpcKind(IParameterSymbol parameter)
	{
		var bytes = parameter.Type is IArrayTypeSymbol { Rank: 1, ElementType.SpecialType: SpecialType.System_Byte };
		return bytes ? "ReadBytes" : Kind(parameter, parameter.Type);
	}

	/// <summary>A value read, as code: an enum through int, a world position into this machine's world.</summary>
	public static string Read(string kind, string type)
	{
		return kind switch
		{
			"enum" => $"({type})reader.ReadInt()",
			WorldPosition => "global::Voxear.Origin.FromLevel(reader.ReadLevelPosition())",
			_ => $"reader.{kind}()",
		};
	}

	/// <summary>A value to write, as code: an enum as int, a world position from the level's middle.</summary>
	public static string Written(string kind, string value)
	{
		return kind switch
		{
			"enum" => $"(int){value}",
			WorldPosition => $"global::Voxear.Origin.ToLevel({value})",
			_ => value,
		};
	}

	public static string Accessibility(Accessibility accessibility)
	{
		return accessibility switch
		{
			Microsoft.CodeAnalysis.Accessibility.Public => "public",
			Microsoft.CodeAnalysis.Accessibility.Internal => "internal",
			Microsoft.CodeAnalysis.Accessibility.Protected => "protected",
			Microsoft.CodeAnalysis.Accessibility.ProtectedOrInternal => "protected internal",
			Microsoft.CodeAnalysis.Accessibility.ProtectedAndInternal => "private protected",
			_ => "private",
		};
	}
}
