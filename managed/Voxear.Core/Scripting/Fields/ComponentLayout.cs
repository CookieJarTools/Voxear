using System;
using System.Collections.Generic;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace Voxear.Scripting
{
	/// <summary>
	/// Where each of a component's fields lies in its bytes, so the editor can read and write a running game's values
	/// in place. .NET has no way to ask for a field's offset, so the layout is worked out as the runtime lays out a
	/// sequential struct, each field at the next multiple of its alignment, and trusted only when it comes to the
	/// struct's real size. An explicit layout (FieldOffset) is read as written.
	/// </summary>
	internal static class ComponentLayout
	{
		private const BindingFlags Instance = BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic;

		private static readonly MethodInfo sizeOf = typeof(Unsafe).GetMethod(nameof(Unsafe.SizeOf))!;

		/// <summary>Each field's offset, or null when the layout cannot be trusted.</summary>
		public static Dictionary<FieldInfo, int>? Of(Type type)
		{
			if (type.IsAutoLayout)
			{
				return null;
			}

			var offsets = new Dictionary<FieldInfo, int>();
			var end = 0;
			var alignment = 1;
			foreach (var field in type.GetFields(Instance))
			{
				var (size, align) = SizeAndAlignment(field.FieldType);
				var explicitAt = field.GetCustomAttribute<FieldOffsetAttribute>()?.Value;
				var offset = explicitAt ?? Align(end, align);
				offsets[field] = offset;
				end = Math.Max(end, offset + size);
				alignment = Math.Max(alignment, align);
			}
			return Align(end, alignment) == RealSize(type) ? offsets : null;
		}

		/// <summary>How many bytes a value takes, and whether a whole number or an enum's value is signed.</summary>
		public static (int Size, bool Signed) Value(Type type)
		{
			var plain = type.IsEnum ? Enum.GetUnderlyingType(type) : type;
			var signed = plain == typeof(sbyte) || plain == typeof(short) || plain == typeof(int) || plain == typeof(long);
			return (RealSize(type), signed);
		}

		private static (int Size, int Alignment) SizeAndAlignment(Type type)
		{
			var size = RealSize(type);
			if (type.IsPrimitive || type.IsEnum || type.IsPointer)
			{
				return (size, size);
			}

			var alignment = 1;
			foreach (var field in type.GetFields(Instance))
			{
				alignment = Math.Max(alignment, SizeAndAlignment(field.FieldType).Alignment);
			}
			return (size, alignment);
		}

		private static int RealSize(Type type)
		{
			return (int)sizeOf.MakeGenericMethod(type).Invoke(null, null)!;
		}

		private static int Align(int at, int alignment)
		{
			return (at + alignment - 1) / alignment * alignment;
		}
	}
}
