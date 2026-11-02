using System.Collections.Generic;
using System.Linq;
using Microsoft.CodeAnalysis;

namespace Voxear.Analyzers;

/// <summary>Finds a type's members by name, its base types' included, as the editor and Voxear.Core look them up.</summary>
internal static class Members
{
	public static IEnumerable<ISymbol> Named(ITypeSymbol type, string name)
	{
		for (var current = type; current != null; current = current.BaseType)
		{
			foreach (var member in current.GetMembers(name))
			{
				yield return member;
			}
		}
	}

	public static bool HasField(ITypeSymbol type, string name)
	{
		return Named(type, name).OfType<IFieldSymbol>().Any();
	}

	/// <summary>Whether a type gives a list of values: an array, or anything enumerable but text.</summary>
	public static bool IsList(ITypeSymbol type)
	{
		if (type is IArrayTypeSymbol)
		{
			return true;
		}
		return type.SpecialType != SpecialType.System_String
			&& (type.SpecialType == SpecialType.System_Collections_IEnumerable
				|| type.AllInterfaces.Any(i => i.SpecialType == SpecialType.System_Collections_IEnumerable));
	}

	public static bool IsNumber(ITypeSymbol type)
	{
		switch (type.SpecialType)
		{
			case SpecialType.System_Single:
			case SpecialType.System_Double:
			case SpecialType.System_Byte:
			case SpecialType.System_SByte:
			case SpecialType.System_Int16:
			case SpecialType.System_UInt16:
			case SpecialType.System_Int32:
			case SpecialType.System_UInt32:
			case SpecialType.System_Int64:
			case SpecialType.System_UInt64:
				return true;
			default:
				return false;
		}
	}
}
