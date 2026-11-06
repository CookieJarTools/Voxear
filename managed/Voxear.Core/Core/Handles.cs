using System;
using System.Collections.Generic;
using System.Numerics;

namespace Voxear;

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
	private const int DiscKind = 4;
	private const float ArrowLength = 1f;

	private static readonly List<HandleShown> shown = new List<HandleShown>();
	private static ulong entity;
	private static string type = string.Empty;
	private static int next;
	private static HandleDrag? drag;
	private static HandleClick? clicked;

	/// <summary>
	/// A point to drag in the plane of the view, given back where it was dragged to. With arrows, also an arrow a metre
	/// long along each axis, red, green and blue, as the move tool's, to drag it along that axis alone.
	/// </summary>
	public static Vector3 Position(Vector3 position, Color? color = null, bool arrows = false)
	{
		var now = Dragged() is { } moved ? moved.Point : position;
		Show(PositionKind, now, Vector3.Zero, 0f, color);
		if (!arrows)
		{
			return now;
		}
		now = Arrow(now, Vector3.UnitX, Voxear.Color.Red);
		now = Arrow(now, Vector3.UnitY, Voxear.Color.Green);
		return Arrow(now, Vector3.UnitZ, Voxear.Color.Blue);
	}

	/// <summary>
	/// A ring round axis through center, radius across, with a dot on it to drag round: gives back rotation turned
	/// round axis as far as the dot was. The dot shows which way rotation faces round the ring. Ctrl turns it in 15°
	/// steps.
	/// </summary>
	public static Quaternion Disc(Vector3 center, Quaternion rotation, Vector3 axis, float radius = 1f, Color? color = null)
	{
		var round = axis.LengthSquared() > 1e-12f ? Vector3.Normalize(axis) : Vector3.UnitY;
		var toward = Toward(rotation, round);
		var now = rotation;
		if (Dragged() is { } moved && Flat(moved.Point - center, round) is { } to)
		{
			var angle = MathF.Atan2(Vector3.Dot(Vector3.Cross(toward, to), round), Vector3.Dot(toward, to));
			now = Quaternion.Normalize(Quaternion.CreateFromAxisAngle(round, angle) * rotation);
			toward = Toward(now, round);
		}
		Show(DiscKind, center, round, radius, color, toward);
		Draw.Circle(center, round, radius, color);
		Draw.Line(center, center + toward * radius, color);
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

	private static void Show(int kind, Vector3 at, Vector3 direction, float number, Color? color, Vector3 toward = default)
	{
		shown.Add(new HandleShown(entity, type, next++, kind, at, direction, number, toward, color));
	}

	/// <summary>An arrow from position along axis, dragged along it, giving back where that leaves position.</summary>
	private static Vector3 Arrow(Vector3 position, Vector3 axis, Color color)
	{
		var now = Dragged() is { } moved ? moved.Point - axis * ArrowLength : position;
		Show(SliderKind, now, axis, ArrowLength, color);
		Draw.Arrow(now, now + axis * ArrowLength, color);
		return now;
	}

	/// <summary>Which way rotation faces round a ring about axis: its own turn of a way across the axis, laid flat in the ring.</summary>
	private static Vector3 Toward(Quaternion rotation, Vector3 axis)
	{
		// Round Y, the rotation's right.
		var across = Vector3.Normalize(Vector3.Cross(axis, MathF.Abs(axis.Z) < 0.9f ? Vector3.UnitZ : Vector3.UnitY));
		return Flat(Vector3.Transform(across, rotation), axis)
			?? Flat(Vector3.Transform(Vector3.Cross(axis, across), rotation), axis)
			?? across;
	}

	/// <summary>A way laid flat across axis and made a unit long, or null when it runs along the axis.</summary>
	private static Vector3? Flat(Vector3 way, Vector3 axis)
	{
		var flat = way - axis * Vector3.Dot(way, axis);
		return flat.LengthSquared() > 1e-8f ? Vector3.Normalize(flat) : null;
	}

	/// <summary>The drag of the handle about to be shown, if the pointer holds it.</summary>
	private static HandleDrag? Dragged()
	{
		return drag is { } held && held.Entity == entity && held.Type == type && held.Id == next ? held : null;
	}
}
