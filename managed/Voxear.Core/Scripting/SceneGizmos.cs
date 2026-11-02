using System;
using System.Collections.Generic;
using System.Reflection;
using System.Text.Json;

namespace Voxear.Scripting
{
	/// <summary>
	/// Behaviours' gizmos, which the editor asks for each frame. In edit mode a behaviour is only its scene's values, so
	/// a copy of each that draws gizmos is kept, bound to its Obj, and filled again only when its values change:
	/// { "action": "gizmos", "behaviours": [{ "behaviour", "entity", "fields", "selected", "hovered" }] }. While the
	/// game plays, the running behaviours draw theirs: { "action": "live-gizmos", "selected", "hovered" }, by entity.
	/// What they draw is marked as gizmos. A behaviour whose gizmos throw is logged once, not every frame.
	/// </summary>
	internal static class SceneGizmos
	{
		private static readonly Dictionary<(ulong Entity, string Type), (string Fields, Behaviour Copy)> kept =
			new Dictionary<(ulong, string), (string, Behaviour)>();
		private static readonly HashSet<string> reported = new HashSet<string>();

		public static void DrawKept(JsonElement request)
		{
			var seen = new HashSet<(ulong, string)>();
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
					Run(copy.Copy, asked.GetProperty("selected").GetBoolean(), asked.GetProperty("hovered").GetBoolean());
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
		}

		public static void DrawLive(JsonElement request)
		{
			var selected = request.GetProperty("selected").GetUInt64();
			var hovered = request.GetProperty("hovered").GetUInt64();
			using (Draw.Gizmos())
			{
				foreach (var behaviour in BehaviourTable.Live())
				{
					if (!behaviour.IsDestroyed && Behaviour.DrawsGizmos(behaviour.GetType()))
					{
						var entity = behaviour.Obj.Entity.Bits;
						Run(behaviour, entity == selected, entity == hovered);
					}
				}
			}
		}

		/// <summary>Lets go of every copy, as when the scripts reload, so the old scripts can leave memory.</summary>
		public static void Forget()
		{
			kept.Clear();
			reported.Clear();
		}

		private static void Run(Behaviour behaviour, bool selected, bool hovered)
		{
			try
			{
				behaviour.RunGizmos(new Gizmo(selected, hovered));
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
}
