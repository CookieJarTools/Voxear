using System;
using System.Collections;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Text;
using System.Text.Json;

namespace Voxear.Scripting
{
	/// <summary>
	/// Runs a behaviour's code for the Inspector in edit mode, when the behaviour has no instance: a [Button] or
	/// [OnValueChanged] method, a [ValidateInput] method, or a [ValueDropdown] member. Each call makes a copy of the
	/// behaviour, sets the scene's values on it and binds it to its Obj, but gives it no Awake or other callback, asks
	/// it, and lets it go. Requests and answers are JSON:
	/// { "behaviour", "entity", "fields", "action": "call" | "validate" | "dropdown", "member", "field" }, answered with
	/// { "changed": { field: value } }, { "valid", "message" } or { "options": [{ "label", "value" }] }, and
	/// { "error" } when it could not be asked, which is logged. The Inspector's drawers are opened and drawn through
	/// here too, as InspectorDrawers says, and the running game's behaviours read and changed while it plays, as
	/// LiveFields says.
	/// </summary>
	internal static class InspectorCalls
	{
		private const BindingFlags Members = BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic;

		public static string Answer(string request)
		{
			using var document = JsonDocument.Parse(request);
			var root = document.RootElement;
			var stream = new MemoryStream();
			using (var json = new Utf8JsonWriter(stream))
			{
				json.WriteStartObject();
				try
				{
					Ask(json, root);
				}
				catch (Exception exception)
				{
					var inner = exception is TargetInvocationException { InnerException: { } thrown } ? thrown : exception;
					Log.Exception(inner);
					json.WriteString("error", inner.Message);
				}
				json.WriteEndObject();
			}
			return Encoding.UTF8.GetString(stream.ToArray());
		}

		private static void Ask(Utf8JsonWriter json, JsonElement root)
		{
			var action = root.GetProperty("action").GetString();
			if (action == "open")
			{
				InspectorDrawers.Open(json, root);
				return;
			}
			if (action == "draw")
			{
				InspectorDrawers.Draw(json, root);
				return;
			}
			if (action is "live-read" or "live-write" or "live-call")
			{
				Live(json, root, action);
				return;
			}

			var behaviour = Make(root);
			var member = root.GetProperty("member").GetString() ?? string.Empty;
			switch (action)
			{
				case "call":
					Call(json, behaviour, member);
					break;
				case "validate":
					Validate(json, behaviour, member, root.GetProperty("field").GetString() ?? string.Empty);
					break;
				case "dropdown":
					Dropdown(json, behaviour, member);
					break;
				default:
					throw new ArgumentException("The Inspector asked for something it cannot.");
			}
		}

		/// <summary>What is asked of the running game's behaviours while it plays: see LiveFields.</summary>
		private static void Live(Utf8JsonWriter json, JsonElement root, string action)
		{
			switch (action)
			{
				case "live-read":
					LiveFields.Read(json, root);
					break;
				case "live-write":
					LiveFields.Write(root);
					break;
				default:
					LiveFields.Call(root);
					break;
			}
		}

		/// <summary>A copy of the behaviour with the scene's values, bound to its Obj.</summary>
		private static Behaviour Make(JsonElement root)
		{
			var name = root.GetProperty("behaviour").GetString() ?? string.Empty;
			var type = Find(name) ?? throw new ArgumentException($"There is no behaviour {name}.");
			var behaviour = (Behaviour)Activator.CreateInstance(type, nonPublic: true)!;
			behaviour.Attach(new Obj(root.GetProperty("entity").GetUInt64()), 0, 0);
			if (root.TryGetProperty("fields", out var fields) && fields.ValueKind == JsonValueKind.Object)
			{
				BehaviourFields.Fill(behaviour, fields, $"{type.Name} on {behaviour.Obj.Name}");
			}
			return behaviour;
		}

		private static Type? Find(string name)
		{
			foreach (var assembly in GameScripts.Assemblies)
			{
				var type = assembly.GetType(name);
				if (type != null && typeof(Behaviour).IsAssignableFrom(type))
				{
					return type;
				}
			}
			return null;
		}

		/// <summary>Calls a method that takes nothing, and answers with the fields it changed, as scenes write them.</summary>
		private static void Call(Utf8JsonWriter json, Behaviour behaviour, string name)
		{
			var method = behaviour.GetType().GetMethod(name, Members, Type.EmptyTypes)
				?? throw new ArgumentException($"{behaviour.GetType().Name} has no method {name} that takes nothing.");
			var before = Values(behaviour);
			method.Invoke(behaviour, null);
			json.WriteStartObject("changed");
			foreach (var (field, value) in Values(behaviour))
			{
				if (before[field] != value)
				{
					json.WritePropertyName(field);
					json.WriteRawValue(value);
				}
			}
			json.WriteEndObject();
		}

		/// <summary>Asks a method whether a field's value is right: it answers bool, or string, empty when it is.</summary>
		private static void Validate(Utf8JsonWriter json, Behaviour behaviour, string name, string fieldName)
		{
			var field = BehaviourFields.Settable(behaviour.GetType()).Find(f => f.Name == fieldName)
				?? throw new ArgumentException($"{behaviour.GetType().Name} has no field {fieldName}.");
			var method = behaviour.GetType().GetMethod(name, Members, new[] { field.FieldType })
				?? throw new ArgumentException($"{behaviour.GetType().Name} has no method {name} that takes a {field.FieldType.Name}.");
			var answer = method.Invoke(behaviour, new[] { field.GetValue(behaviour) });
			var message = answer as string ?? string.Empty;
			json.WriteBoolean("valid", answer is bool valid ? valid : message.Length == 0);
			json.WriteString("message", message);
		}

		/// <summary>The values a member offers, each with its name: a ValueDropdownItem's text, or the value's own.</summary>
		private static void Dropdown(Utf8JsonWriter json, Behaviour behaviour, string name)
		{
			var type = behaviour.GetType();
			var offered = type.GetMethod(name, Members, Type.EmptyTypes)?.Invoke(behaviour, null)
				?? type.GetProperty(name, Members)?.GetValue(behaviour)
				?? type.GetField(name, Members)?.GetValue(behaviour);
			if (offered is not IEnumerable items)
			{
				throw new ArgumentException($"{type.Name}.{name} gives no list of values.");
			}

			json.WriteStartArray("options");
			foreach (var item in items)
			{
				var (label, value) = item is ValueDropdownItem named ? (named.Text, named.Value) : (item?.ToString() ?? "None", item);
				json.WriteStartObject();
				json.WriteString("label", label);
				json.WritePropertyName("value");
				FieldDefaults.Write(json, value, value?.GetType() ?? typeof(object));
				json.WriteEndObject();
			}
			json.WriteEndArray();
		}

		/// <summary>Each field scenes can set, as a scene writes its value, to tell what a call changed.</summary>
		private static Dictionary<string, string> Values(Behaviour behaviour)
		{
			var values = new Dictionary<string, string>();
			foreach (var field in BehaviourFields.Settable(behaviour.GetType()))
			{
				var stream = new MemoryStream();
				using (var json = new Utf8JsonWriter(stream))
				{
					FieldDefaults.Write(json, field.GetValue(behaviour), field.FieldType);
				}
				values[field.Name] = Encoding.UTF8.GetString(stream.ToArray());
			}
			return values;
		}
	}
}
