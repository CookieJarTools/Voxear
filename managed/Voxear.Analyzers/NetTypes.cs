using Microsoft.CodeAnalysis;

namespace Voxear.Analyzers;

/// <summary>The types a NetWriter writes and a NetReader reads, for replicated properties and RPCs' arguments.</summary>
internal static class NetTypes
{
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

	/// <summary>How an RPC's argument of this type is read: as <see cref="Kind"/> says, and bytes too, which change too much to replicate.</summary>
	public static string? RpcKind(ITypeSymbol type)
	{
		var bytes = type is IArrayTypeSymbol { Rank: 1, ElementType.SpecialType: SpecialType.System_Byte };
		return bytes ? "ReadBytes" : Kind(type);
	}

	/// <summary>A value read, as code: an enum through int.</summary>
	public static string Read(string kind, string type)
	{
		return kind == "enum" ? $"({type})reader.ReadInt()" : $"reader.{kind}()";
	}

	/// <summary>A value to write, as code: an enum as int.</summary>
	public static string Written(string kind, string value)
	{
		return kind == "enum" ? $"(int){value}" : value;
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
