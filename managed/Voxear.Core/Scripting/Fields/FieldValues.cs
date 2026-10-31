using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using System.Numerics;
using System.Reflection;
using System.Text.Json;

namespace Voxear.Scripting
{
	/// <summary>
	/// Turns field values from scenes and prefabs, which arrive as JSON, into the types behaviour fields declare. A
	/// value that does not fit throws, and BehaviourFields reports it as a warning naming the field. path says where
	/// the value sits, for warnings about values nested inside it.
	/// </summary>
	internal static class FieldValues
	{
		private const float DegreesToRadians = MathF.PI / 180f;

		public static object? Read(JsonElement value, Type type, string path)
		{
			if (type == typeof(string))
			{
				return value.GetString();
			}
			if (type == typeof(bool))
			{
				return value.GetBoolean();
			}
			if (type.IsEnum)
			{
				return ReadEnum(value, type);
			}
			if (IsInteger(type))
			{
				return Convert.ChangeType(value.GetInt64(), type, CultureInfo.InvariantCulture);
			}
			if (type == typeof(float) || type == typeof(double))
			{
				return Convert.ChangeType(value.GetDouble(), type, CultureInfo.InvariantCulture);
			}
			return ReadStructured(value, type, path);
		}

		private static object? ReadStructured(JsonElement value, Type type, string path)
		{
			if (type == typeof(Vector2))
			{
				var v = Floats(value, 2);
				return new Vector2(v[0], v[1]);
			}
			if (type == typeof(Vector3))
			{
				var v = Floats(value, 3);
				return new Vector3(v[0], v[1], v[2]);
			}
			if (type == typeof(Vector4))
			{
				var v = Floats(value, 4);
				return new Vector4(v[0], v[1], v[2], v[3]);
			}
			if (type == typeof(Quaternion))
			{
				return ReadRotation(value);
			}
			if (type == typeof(Obj))
			{
				return ReadObj(value);
			}
			return ReadAssetOrList(value, type, path);
		}

		private static object? ReadAssetOrList(JsonElement value, Type type, string path)
		{
			if (IsGeneric(type, typeof(Ref<>)))
			{
				return Activator.CreateInstance(type, value.GetString() ?? string.Empty);
			}
			if (IsAsset(type))
			{
				return type.GetMethod("Load", new[] { typeof(string) })!.Invoke(null, new object?[] { value.GetString() });
			}
			if (type.IsArray)
			{
				return ReadArray(value, type.GetElementType()!, path);
			}
			if (IsGeneric(type, typeof(List<>)))
			{
				return ReadList(value, type, path);
			}
			return ReadReferenceOrNested(value, type, path);
		}

		private static object? ReadReferenceOrNested(JsonElement value, Type type, string path)
		{
			if (typeof(Behaviour).IsAssignableFrom(type) || type.IsInterface)
			{
				return ReadBehaviour(value, type);
			}
			if (IsSerializable(type))
			{
				return ReadNested(value, type, path);
			}
			throw new NotSupportedException($"Fields of type {type.Name} cannot be set from scenes. Mark {type.Name} [Serializable] to set its fields as a table.");
		}

		/// <summary>
		/// A behaviour on another Obj, written as that Obj: { obj = 3 }. It is the Obj's first behaviour of the
		/// field's type, or that implements the field's interface. Every behaviour of a scene exists before any Awake,
		/// so it is always there to find.
		/// </summary>
		private static Behaviour? ReadBehaviour(JsonElement value, Type type)
		{
			var obj = ReadObj(value);
			if (!obj.IsValid)
			{
				return null;
			}
			return obj.GetBehaviour(type) ?? throw new FormatException($"{obj.Name} has no {type.Name}.");
		}

		/// <summary>A [Serializable] class or struct, written as a table of its fields.</summary>
		private static object ReadNested(JsonElement value, Type type, string path)
		{
			if (value.ValueKind != JsonValueKind.Object)
			{
				throw new FormatException($"A {type.Name} is written as a table, such as {{ name = value }}.");
			}

			var nested = Activator.CreateInstance(type)!;
			BehaviourFields.Fill(nested, value, path);
			return nested;
		}

		/// <summary>Euler degrees applied Z, then X, then Y, like transforms in scenes, or four quaternion components.</summary>
		private static Quaternion ReadRotation(JsonElement value)
		{
			if (value.GetArrayLength() == 4)
			{
				var q = Floats(value, 4);
				return Quaternion.Normalize(new Quaternion(q[0], q[1], q[2], q[3]));
			}

			var euler = Floats(value, 3);
			return Quaternion.CreateFromYawPitchRoll(euler[1] * DegreesToRadians, euler[0] * DegreesToRadians, euler[2] * DegreesToRadians);
		}

		/// <summary>
		/// Scenes write { obj = 3 }, and the engine hands over the entity it became as { entity = n }. An empty table is
		/// a reference whose Obj was not saved with the scene, and reads as None.
		/// </summary>
		private static Obj ReadObj(JsonElement value)
		{
			if (value.TryGetProperty("entity", out var entity))
			{
				return new Obj((ulong)entity.GetInt64());
			}
			if (value.ValueKind == JsonValueKind.Object && !value.EnumerateObject().MoveNext())
			{
				return Obj.None;
			}
			throw new FormatException("An Obj field takes { obj = 3 }: an Obj in the same scene or prefab.");
		}

		private static object ReadEnum(JsonElement value, Type type)
		{
			if (value.ValueKind == JsonValueKind.Number)
			{
				return Enum.ToObject(type, value.GetInt64());
			}

			var wanted = BehaviourFields.Normalize(value.GetString() ?? string.Empty);
			foreach (var name in Enum.GetNames(type))
			{
				if (BehaviourFields.Normalize(name) == wanted)
				{
					return Enum.Parse(type, name);
				}
			}
			throw new FormatException($"{type.Name} has no value named {value.GetString()}; it has {string.Join(", ", Enum.GetNames(type))}.");
		}

		private static Array ReadArray(JsonElement value, Type elementType, string path)
		{
			var array = Array.CreateInstance(elementType, value.GetArrayLength());
			var index = 0;
			foreach (var item in value.EnumerateArray())
			{
				array.SetValue(Read(item, elementType, $"{path}[{index}]"), index);
				index++;
			}
			return array;
		}

		private static IList ReadList(JsonElement value, Type listType, string path)
		{
			var list = (IList)Activator.CreateInstance(listType)!;
			var elementType = listType.GetGenericArguments()[0];
			foreach (var item in value.EnumerateArray())
			{
				list.Add(Read(item, elementType, $"{path}[{list.Count}]"));
			}
			return list;
		}

		private static float[] Floats(JsonElement value, int count)
		{
			if (value.GetArrayLength() != count)
			{
				throw new FormatException($"It needs {count} numbers.");
			}

			var floats = new float[count];
			var index = 0;
			foreach (var item in value.EnumerateArray())
			{
				floats[index++] = item.GetSingle();
			}
			return floats;
		}

		internal static bool IsInteger(Type type)
		{
			return type == typeof(int) || type == typeof(uint) || type == typeof(long) || type == typeof(ulong)
				|| type == typeof(short) || type == typeof(ushort) || type == typeof(byte) || type == typeof(sbyte);
		}

		/// <summary>
		/// Whether the type is marked [Serializable], as Unity marks types whose fields scenes fill. Only formatter
		/// serialization is obsolete, not the mark, so the warning about reading it does not apply.
		/// </summary>
		internal static bool IsSerializable(Type type)
		{
#pragma warning disable SYSLIB0050
			return type.Attributes.HasFlag(TypeAttributes.Serializable) && !type.IsPrimitive;
#pragma warning restore SYSLIB0050
		}

		internal static bool IsGeneric(Type type, Type definition)
		{
			return type.IsGenericType && type.GetGenericTypeDefinition() == definition;
		}

		/// <summary>
		/// Whether the type is an IAsset of itself. Asking through its interfaces, rather than MakeGenericType, does not
		/// throw for the many structs that fail IAsset's constraint.
		/// </summary>
		internal static bool IsAsset(Type type)
		{
			foreach (var contract in type.GetInterfaces())
			{
				if (IsGeneric(contract, typeof(IAsset<>)) && contract.GetGenericArguments()[0] == type)
				{
					return true;
				}
			}
			return false;
		}
	}
}
