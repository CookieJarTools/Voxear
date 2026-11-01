using System;
using System.Collections.Generic;
using System.Reflection;
using System.Text.Json;

namespace Voxear.Scripting
{
	/// <summary>
	/// The running game's behaviours as the Inspector shows and edits them while it plays: each field scenes can set,
	/// read from the instance as a scene writes it; a value the Inspector changed, set on the instance; and a [Button]
	/// or [OnValueChanged] method, called on it. Requests and answers are JSON, through InspectorCalls:
	/// { "action": "live-read", "handles" }, answered with { "behaviours": [{ field: value } or null for one gone] };
	/// { "action": "live-write", "handle", "field", "value" }; and { "action": "live-call", "handle", "method" }.
	/// </summary>
	internal static class LiveFields
	{
		private const BindingFlags Members = BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic;

		public static void Read(Utf8JsonWriter json, JsonElement request)
		{
			json.WriteStartArray("behaviours");
			foreach (var handle in request.GetProperty("handles").EnumerateArray())
			{
				var behaviour = BehaviourTable.Get(handle.GetUInt32());
				if (behaviour == null)
				{
					json.WriteNullValue();
					continue;
				}
				WriteFields(json, behaviour);
			}
			json.WriteEndArray();
		}

		public static void Write(JsonElement request)
		{
			var behaviour = Find(request);
			var name = BehaviourFields.Normalize(request.GetProperty("field").GetString() ?? string.Empty);
			var field = BehaviourFields.Settable(behaviour.GetType()).Find(f => BehaviourFields.Normalize(f.Name) == name)
				?? throw new ArgumentException($"{behaviour.GetType().Name} has no field {request.GetProperty("field").GetString()}.");
			var given = request.GetProperty("value");
			var value = given.ValueKind == JsonValueKind.Null ? null : FieldValues.Read(given, field.FieldType, field.Name);
			field.SetValue(behaviour, value);
		}

		public static void Call(JsonElement request)
		{
			var behaviour = Find(request);
			var name = request.GetProperty("method").GetString() ?? string.Empty;
			var method = behaviour.GetType().GetMethod(name, Members, Type.EmptyTypes)
				?? throw new ArgumentException($"{behaviour.GetType().Name} has no method {name} that takes nothing.");
			method.Invoke(behaviour, null);
		}

		private static Behaviour Find(JsonElement request)
		{
			var handle = request.GetProperty("handle").GetUInt32();
			return BehaviourTable.Get(handle) ?? throw new ArgumentException("That behaviour is no longer running.");
		}

		private static void WriteFields(Utf8JsonWriter json, Behaviour behaviour)
		{
			json.WriteStartObject();
			foreach (var field in BehaviourFields.Settable(behaviour.GetType()))
			{
				json.WritePropertyName(field.Name);
				FieldDefaults.Write(json, field.GetValue(behaviour), field.FieldType);
			}
			json.WriteEndObject();
		}
	}
}
