using System.Collections.Generic;
using System.Numerics;

namespace Voxear
{
	/// <summary>
	/// Handles to drag in the Scene view, for tools of your own, called from a behaviour's DrawGizmos. Each takes a value
	/// and gives back the value as the drag has left it, so assigning it back is all there is to do: the editor writes the
	/// change into the scene as one step to undo. While the game plays, it changes the running behaviour.
	/// <code>
	/// protected override void DrawGizmos(Gizmo gizmo)
	/// {
	///     if (gizmo.IsSelected)
	///     {
	///         Target = Handles.Position(Target);
	///         Range = Handles.Radius(Obj.WorldPosition, Range);
	///     }
	/// }
	/// </code>
	/// Points are in the world. A handle is told apart from the others by the order DrawGizmos calls it in, so call them
	/// in the same order each time.
	/// </summary>
	public static class Handles
	{
		private const int PositionKind = 0;
		private const int RadiusKind = 1;
		private const int SliderKind = 2;
		private const int ButtonKind = 3;

		private static readonly List<HandleShown> shown = new List<HandleShown>();
		private static ulong entity;
		private static string type = string.Empty;
		private static int next;
		private static HandleDrag? drag;
		private static HandleClick? clicked;

		/// <summary>A point to drag in the plane of the view, given back where it was dragged to.</summary>
		public static Vector3 Position(Vector3 position, Color? color = null)
		{
			var now = Dragged() is { } moved ? moved.Point : position;
			Show(PositionKind, now, Vector3.Zero, 0f, color);
			return now;
		}

		/// <summary>A sphere around center with a point on its edge, dragged to grow or shrink it, given back as its new radius.</summary>
		public static float Radius(Vector3 center, float radius, Color? color = null)
		{
			var now = Dragged() is { } moved ? moved.Number : radius;
			Show(RadiusKind, center, Vector3.Zero, now, color);
			Draw.WireSphere(center, now, color);
			return now;
		}

		/// <summary>A point along a line from origin in direction, dragged along it, given back as how far along it is.</summary>
		public static float Slider(Vector3 origin, Vector3 direction, float distance, Color? color = null)
		{
			var along = direction.LengthSquared() > 1e-12f ? Vector3.Normalize(direction) : Vector3.UnitY;
			var now = Dragged() is { } moved ? moved.Number : distance;
			Show(SliderKind, origin, along, now, color);
			Draw.Line(origin, origin + along * now, color);
			return now;
		}

		/// <summary>A button in the Scene view at a point: true the frame it is clicked.</summary>
		public static bool Button(Vector3 position, Color? color = null)
		{
			var mine = clicked == new HandleClick(entity, type, next);
			Show(ButtonKind, position, Vector3.Zero, 0f, color);
			return mine;
		}

		/// <summary>Starts a frame's handles: what the editor's pointer is dragging, or clicked.</summary>
		internal static void BeginFrame(HandleDrag? dragging, HandleClick? click)
		{
			shown.Clear();
			drag = dragging;
			clicked = click;
		}

		/// <summary>Starts one behaviour's handles, counted from the first again.</summary>
		internal static void BeginOwner(ulong owner, string ownerType)
		{
			entity = owner;
			type = ownerType;
			next = 0;
		}

		/// <summary>Whether the pointer is on one of this behaviour's handles, so what it changes is the pointer's doing.</summary>
		internal static bool Holds(ulong owner, string ownerType)
		{
			return (drag is { } held && held.Entity == owner && held.Type == ownerType)
				|| (clicked is { } click && click.Entity == owner && click.Type == ownerType);
		}

		/// <summary>The handles shown this frame, for the editor to draw and take hold of.</summary>
		internal static IReadOnlyList<HandleShown> Shown => shown;

		private static void Show(int kind, Vector3 at, Vector3 direction, float number, Color? color)
		{
			shown.Add(new HandleShown(entity, type, next++, kind, at, direction, number, color));
		}

		/// <summary>The drag of the handle about to be shown, if the pointer holds it.</summary>
		private static HandleDrag? Dragged()
		{
			return drag is { } held && held.Entity == entity && held.Type == type && held.Id == next ? held : null;
		}
	}
}
