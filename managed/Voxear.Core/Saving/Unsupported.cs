using System;
using System.IO;
using System.Reflection;
using System.Threading.Tasks;

namespace Voxear.Saving
{
	/// <summary>The types a save refuses, because what they hold means nothing in another run of the game.</summary>
	internal static class Unsupported
	{
		/// <summary>Why a type cannot be saved, or null when it can.</summary>
		public static string? Why(Type type)
		{
			if (typeof(Delegate).IsAssignableFrom(type))
			{
				return "a delegate is code, not a value, so it cannot be saved";
			}
			if (type.IsPointer || type == typeof(IntPtr) || type == typeof(UIntPtr))
			{
				return "a pointer means nothing in another run of the game";
			}
			if (type == typeof(Entity))
			{
				return "an Entity's number means nothing in another run of the game; save the values its components hold";
			}
			if (typeof(MemberInfo).IsAssignableFrom(type) || typeof(Stream).IsAssignableFrom(type) || typeof(Task).IsAssignableFrom(type))
			{
				return $"a {type.Name} cannot be saved";
			}
			if (type.IsArray && type.GetArrayRank() > 1)
			{
				return "an array with more than one dimension cannot be saved; use an array of arrays, or a List of Lists";
			}
			return null;
		}
	}
}
