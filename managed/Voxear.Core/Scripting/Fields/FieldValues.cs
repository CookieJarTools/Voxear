using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using System.Numerics;
using System.Text.Json;

namespace Voxear.Scripting
{
	/// <summary>
	/// Turns field values from scenes and prefabs, which arrive as JSON, into the types behaviour fields declare. A
	/// value that does not fit throws, and BehaviourFields reports it as a warning naming the field.
	/// </summary>
	internal static class FieldValues
	{
		private const float DegreesToRadians = MathF.PI / 180f;

		public static object? Read(JsonElement value, Type type)
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
			return ReadStructured(value, type);
		}

		private static object? ReadStructured(JsonElement value, Type type)
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
			return ReadAssetOrList(value, type);
		}

		private static object? ReadAssetOrList(JsonElement value, Type type)
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
				return ReadArray(value, type.GetElementType()!);
			}
			if (IsGeneric(type, typeof(List<>)))
			{
				return ReadList(value, type);
			}
			throw new NotSupportedException($"Fields of type {type.Name} cannot be set from scenes yet.");
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

		private static Array ReadArray(JsonElement value, Type elementType)
		{
			var array = Array.CreateInstance(elementType, value.GetArrayLength());
			var index = 0;
			foreach (var item in value.EnumerateArray())
			{
				array.SetValue(Read(item, elementType), index++);
			}
			return array;
		}

		private static IList ReadList(JsonElement value, Type listType)
		{
			var list = (IList)Activator.CreateInstance(listType)!;
			var elementType = listType.GetGenericArguments()[0];
			foreach (var item in value.EnumerateArray())
			{
				list.Add(Read(item, elementType));
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

		private static bool IsInteger(Type type)
		{
			return type == typeof(int) || type == typeof(uint) || type == typeof(long) || type == typeof(ulong)
				|| type == typeof(short) || type == typeof(ushort) || type == typeof(byte) || type == typeof(sbyte);
		}

		private static bool IsGeneric(Type type, Type definition)
		{
			return type.IsGenericType && type.GetGenericTypeDefinition() == definition;
		}

		private static bool IsAsset(Type type)
		{
			return type.IsValueType && typeof(IAsset<>).MakeGenericType(type).IsAssignableFrom(type);
		}
	}
}
