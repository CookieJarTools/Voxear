using System;
using System.Collections.Generic;
using System.Numerics;
using System.Reflection;
using System.Text.Json;

namespace Voxear.Scripting;

/// <summary>
/// Behaviours' gizmos, which the editor asks for each frame. In edit mode a behaviour is only its scene's values, so
/// a copy of each that draws gizmos is kept, bound to its Obj, and filled again only when its values change:
/// { "action": "gizmos", "behaviours": [{ "behaviour", "entity", "fields", "selected", "hovered" }] }. While the
/// game plays, the running behaviours draw theirs: { "action": "live-gizmos", "selected", "hovered", "hidden" }, by
/// entity, but for the types the Gizmos menu hides.
/// What they draw is marked as gizmos. A behaviour whose gizmos throw is logged once, not every frame.
/// <para>
/// Both carry what the editor's pointer does to Handles, each named by the behaviour that showed it, by "entity"
/// and "behaviour", and its "id": "drag" with a "point" and a "number", and "clicked". The answer lists the
/// "handles" shown, and, in edit mode, the fields that dragging or clicking one "changed" on a copy, by entity and
/// behaviour, for the editor to write into the scene, after calling each changed field's [OnValueChanged] method,
/// as the Inspector does. What DrawGizmos changes on its own is not kept.
/// </para>
/// <para>
/// The answer also says how many gizmo lines each behaviour "drew", in order, by entity, so a click on a line in
/// the Scene view can select the Obj whose gizmo it is.
/// </para>
/// </summary>
internal static class SceneGizmos
{
	private const BindingFlags Members = BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic;

	private static readonly Dictionary<(ulong Entity, string Type), (string Fields, Behaviour Copy)> kept =
		new Dictionary<(ulong, string), (string, Behaviour)>();
	private static readonly HashSet<string> reported = new HashSet<string>();

	public static void DrawKept(Utf8JsonWriter json, JsonElement request)
	{
		BeginHandles(request);
		var seen = new HashSet<(ulong, string)>();
		var changed = new List<(ulong Entity, string Type, Dictionary<string, string> Fields)>();
		var drawn = new List<(ulong Entity, int Lines)>();
		using (Draw.Gizmos())
		{
			foreach (var asked in request.GetProperty("behaviours").EnumerateArray())
			{
				var key = (asked.GetProperty("entity").GetUInt64(), asked.GetProperty("behaviour").GetString() ?? string.Empty);
				seen.Add(key);
				var fields = asked.TryGetProperty("fields", out var given) ? given.GetRawText() : string.Empty;
				if (!kept.TryGetValue(key, out var copy) || copy.Fields != fields)
				{
					copy = (fields, InspectorCalls.Make(asked));
					kept[key] = copy;
				}
				var selected = asked.GetProperty("selected").GetBoolean();
				var hovered = asked.GetProperty("hovered").GetBoolean();
				var lines = Draw.GizmoLines;
				if (RunHeld(copy.Copy, key, selected, hovered) is { } different)
				{
					changed.Add((key.Item1, key.Item2, different));
				}
				drawn.Add((key.Item1, Draw.GizmoLines - lines));
			}
		}
		// Copies of behaviours no longer asked for, gone or deselected from the scene, are let go.
		foreach (var key in new List<(ulong, string)>(kept.Keys))
		{
			if (!seen.Contains(key))
			{
				kept.Remove(key);
			}
		}
		WriteHandles(json);
		WriteDrawn(json, drawn);
		json.WriteStartArray("changed");
		foreach (var (entity, type, fields) in changed)
		{
			json.WriteStartObject();
			json.WriteNumber("entity", entity);
			json.WriteString("behaviour", type);
			json.WriteStartObject("fields");
			foreach (var (name, value) in fields)
			{
				json.WritePropertyName(name);
				json.WriteRawValue(value);
			}
			json.WriteEndObject();
			json.WriteEndObject();
		}
		json.WriteEndArray();
	}

	public static void DrawLive(Utf8JsonWriter json, JsonElement request)
	{
		BeginHandles(request);
		var selected = request.GetProperty("selected").GetUInt64();
		var hovered = request.GetProperty("hovered").GetUInt64();
		var hidden = new HashSet<string>();
		if (request.TryGetProperty("hidden", out var named))
		{
			foreach (var type in named.EnumerateArray())
			{
				hidden.Add(type.GetString() ?? string.Empty);
			}
		}
		var drawn = new List<(ulong Entity, int Lines)>();
		using (Draw.Gizmos())
		{
			foreach (var behaviour in BehaviourTable.Live())
			{
				if (!behaviour.IsDestroyed && Behaviour.DrawsGizmos(behaviour.GetType()) && !hidden.Contains(behaviour.GetType().FullName ?? string.Empty))
				{
					var entity = behaviour.Obj.Entity.Bits;
					var lines = Draw.GizmoLines;
					RunHeld(behaviour, (entity, behaviour.GetType().FullName ?? string.Empty), entity == selected, entity == hovered);
					drawn.Add((entity, Draw.GizmoLines - lines));
				}
			}
		}
		WriteHandles(json);
		WriteDrawn(json, drawn);
	}

	/// <summary>Lets go of every copy, as when the scripts reload, so the old scripts can leave memory.</summary>
	public static void Forget()
	{
		kept.Clear();
		reported.Clear();
	}

	private static void BeginHandles(JsonElement request)
	{
		HandleDrag? drag = null;
		if (request.TryGetProperty("drag", out var dragging) && dragging.ValueKind == JsonValueKind.Object)
		{
			var point = dragging.GetProperty("point");
			drag = new HandleDrag(
				dragging.GetProperty("entity").GetUInt64(),
				dragging.GetProperty("behaviour").GetString() ?? string.Empty,
				dragging.GetProperty("id").GetInt32(),
				new Vector3(point[0].GetSingle(), point[1].GetSingle(), point[2].GetSingle()),
				dragging.GetProperty("number").GetSingle());
		}
		HandleClick? click = null;
		if (request.TryGetProperty("clicked", out var clicked) && clicked.ValueKind == JsonValueKind.Object)
		{
			click = new HandleClick(
				clicked.GetProperty("entity").GetUInt64(),
				clicked.GetProperty("behaviour").GetString() ?? string.Empty,
				clicked.GetProperty("id").GetInt32());
		}
		Handles.BeginFrame(drag, click);
	}

	private static void WriteHandles(Utf8JsonWriter json)
	{
		json.WriteStartArray("handles");
		foreach (var handle in Handles.Shown)
		{
			json.WriteStartObject();
			json.WriteNumber("entity", handle.Entity);
			json.WriteString("behaviour", handle.Type);
			json.WriteNumber("id", handle.Id);
			json.WriteNumber("kind", handle.Kind);
			WriteVector(json, "at", handle.At);
			WriteVector(json, "direction", handle.Direction);
			json.WriteNumber("number", handle.Number);
			WriteVector(json, "toward", handle.Toward);
			if (handle.Color is { } color)
			{
				json.WriteStartArray("color");
				foreach (var part in (ReadOnlySpan<float>)[color.R, color.G, color.B, color.A])
				{
					json.WriteNumberValue(part);
				}
				json.WriteEndArray();
			}
			json.WriteEndObject();
		}
		json.WriteEndArray();
	}

	private static void WriteDrawn(Utf8JsonWriter json, List<(ulong Entity, int Lines)> drawn)
	{
		json.WriteStartArray("drawn");
		foreach (var (entity, lines) in drawn)
		{
			if (lines > 0)
			{
				json.WriteStartArray();
				json.WriteNumberValue(entity);
				json.WriteNumberValue(lines);
				json.WriteEndArray();
			}
		}
		json.WriteEndArray();
	}

	private static void WriteVector(Utf8JsonWriter json, string name, Vector3 vector)
	{
		json.WriteStartArray(name);
		json.WriteNumberValue(vector.X);
		json.WriteNumberValue(vector.Y);
		json.WriteNumberValue(vector.Z);
		json.WriteEndArray();
	}

	/// <summary>
	/// Runs a behaviour's gizmos. While the pointer holds one of its handles, gives back the fields that changed,
	/// once each changed field's [OnValueChanged] method has run; otherwise, nothing.
	/// </summary>
	private static Dictionary<string, string>? RunHeld(Behaviour behaviour, (ulong Entity, string Type) owner, bool selected, bool hovered)
	{
		var before = Handles.Holds(owner.Entity, owner.Type) ? InspectorCalls.Values(behaviour) : null;
		Handles.BeginOwner(owner.Entity, owner.Type);
		Guarded(behaviour, () => behaviour.RunGizmos(new Gizmo(selected, hovered)));
		if (before == null || Changed(before, InspectorCalls.Values(behaviour)) is not { Count: > 0 } changed)
		{
			return null;
		}
		var methods = new HashSet<string>();
		foreach (var name in changed.Keys)
		{
			if (behaviour.GetType().GetField(name, Members)?.GetCustomAttribute<OnValueChangedAttribute>() is { } callback && methods.Add(callback.Method))
			{
				Guarded(behaviour, () => behaviour.GetType().GetMethod(callback.Method, Members, Type.EmptyTypes)?.Invoke(behaviour, null));
			}
		}
		return Changed(before, InspectorCalls.Values(behaviour));
	}

	private static Dictionary<string, string> Changed(Dictionary<string, string> before, Dictionary<string, string> after)
	{
		var changed = new Dictionary<string, string>();
		foreach (var (name, value) in after)
		{
			if (!before.TryGetValue(name, out var was) || was != value)
			{
				changed[name] = value;
			}
		}
		return changed;
	}

	/// <summary>Runs a behaviour's code for the gizmos, logging what it throws once, not every frame.</summary>
	private static void Guarded(Behaviour behaviour, Action run)
	{
		try
		{
			run();
		}
		catch (Exception exception)
		{
			var inner = exception is TargetInvocationException { InnerException: { } thrown } ? thrown : exception;
			if (reported.Add($"{behaviour.GetType().FullName}: {inner.Message}"))
			{
				Log.Exception(inner);
			}
		}
	}
}
