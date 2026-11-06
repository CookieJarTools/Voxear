using System;
using System.Numerics;
using System.Text;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// Draws lines, shapes and labels in the 3D view from anywhere, at any time: Update, systems, worker threads, or a
/// behaviour's gizmos. A shape lasts one frame unless a duration says otherwise, so code that draws each frame stays
/// up to date. Lines are dimmed where the scene is in front of them, so they stay readable without hiding it.
/// <code>
/// Draw.WireSphere(Obj.WorldPosition, range);
/// using (Draw.Color(Color.Red))
/// using (Draw.Width(3f))
/// {
///     Draw.Arrow(Obj.WorldPosition, target);
/// }
/// </code>
/// <para>
/// Scopes set how everything inside them draws, and put back what was there before when they end: Color, Matrix
/// and InLocalSpace, Width, Duration, OnTop and InGame. Each shape also takes a colour of its own.
/// </para>
/// <para>
/// Drawing shows in the editor and in development builds. A release build leaves it out, all but what is drawn
/// inside Draw.InGame(), which is part of the game. Unlike Debug's calls, which are compiled out of a release
/// build, these are still called there, costing a check each, so keep heavy drawing behind your own setting.
/// </para>
/// </summary>
public static unsafe class Draw
{
	private const int Segments = 32;
	private const int OnTopFlag = 1;
	private const int HeadFlag = 2;
	private const int InGameFlag = 4;
	private const int GizmoFlag = 8;
	private const int MetresFlag = 16;
	private const int WidthShift = 8;

	[ThreadStatic] private static bool started;
	[ThreadStatic] private static DrawState state;

	/// <summary>Where lines go instead of the engine, for tests: from, to, colour, duration and flags.</summary>
	internal static Action<Vector3, Vector3, Color, float, int>? Sink { get; set; }

	/// <summary>How many lines have been drawn as gizmos, so the editor can tell whose gizmo a click is on.</summary>
	[field: ThreadStatic] internal static int GizmoLines { get; private set; }

	private static ref DrawState State
	{
		get
		{
			if (!started)
			{
				state = DrawState.Default;
				started = true;
			}
			return ref state;
		}
	}

	/// <summary>Draws in this colour inside the scope.</summary>
	public static DrawScope Color(Color color)
	{
		var scope = new DrawScope(State);
		State.Color = color;
		return scope;
	}

	/// <summary>Places what is drawn inside the scope by this matrix, within any matrix around it.</summary>
	public static DrawScope Matrix(Matrix4x4 matrix)
	{
		var scope = new DrawScope(State);
		State.Matrix = matrix * State.Matrix;
		return scope;
	}

	/// <summary>Draws in an Obj's own space inside the scope: its position, rotation and scale in the world.</summary>
	public static DrawScope InLocalSpace(Obj obj)
	{
		var world = Native.IsConnected ? Native.GetWorldTransform(obj.Id) : Transform.Identity;
		var matrix = Matrix4x4.CreateScale(world.Scale)
			* Matrix4x4.CreateFromQuaternion(world.Rotation)
			* Matrix4x4.CreateTranslation(world.Position);
		return Matrix(matrix);
	}

	/// <summary>Draws lines this many pixels wide inside the scope. Zero is the usual width.</summary>
	public static DrawScope Width(float pixels)
	{
		var scope = new DrawScope(State);
		State.Width = Math.Clamp(pixels, 0f, 4000f);
		State.WidthInMetres = false;
		return scope;
	}

	/// <summary>
	/// Draws lines this many metres wide inside the scope, up to 65, so they narrow with distance as a rope or a
	/// laser beam does, but never under a pixel.
	/// </summary>
	public static DrawScope WidthInMetres(float metres)
	{
		var scope = new DrawScope(State);
		State.Width = Math.Clamp(metres, 0f, 65f);
		State.WidthInMetres = true;
		return scope;
	}

	/// <summary>Keeps what is drawn inside the scope up for this many seconds of game time, rather than one frame.</summary>
	public static DrawScope Duration(float seconds)
	{
		var scope = new DrawScope(State);
		State.Duration = Math.Max(0f, seconds);
		return scope;
	}

	/// <summary>Draws over the scene inside the scope, rather than dimmed where the scene is in front.</summary>
	public static DrawScope OnTop()
	{
		var scope = new DrawScope(State);
		State.OnTop = true;
		return scope;
	}

	/// <summary>Marks what is drawn inside the scope as part of the game, so it shows in release builds too.</summary>
	public static DrawScope InGame()
	{
		var scope = new DrawScope(State);
		State.InGame = true;
		return scope;
	}

	/// <summary>Marks what is drawn inside the scope as a behaviour's gizmos, which only views showing gizmos draw.</summary>
	internal static DrawScope Gizmos()
	{
		var scope = new DrawScope(State);
		State.Gizmo = true;
		return scope;
	}

	public static void Line(Vector3 from, Vector3 to, Color? color = null)
	{
		Segment(from, to, color, 0);
	}

	/// <summary>A line from origin along direction, as far as direction is long.</summary>
	public static void Ray(Vector3 origin, Vector3 direction, Color? color = null)
	{
		Segment(origin, origin + direction, color, 0);
	}

	/// <summary>A line with a head at its end, as for a direction or a velocity.</summary>
	public static void Arrow(Vector3 from, Vector3 to, Color? color = null)
	{
		Segment(from, to, color, HeadFlag);
	}

	/// <summary>Lines from each pair of points: the first to the second, the third to the fourth, and so on.</summary>
	public static void Lines(ReadOnlySpan<Vector3> pairs, Color? color = null)
	{
		for (var i = 0; i + 1 < pairs.Length; i += 2)
		{
			Segment(pairs[i], pairs[i + 1], color, 0);
		}
	}

	/// <summary>Lines through the points in order, and back to the first when closed.</summary>
	public static void Polyline(ReadOnlySpan<Vector3> points, bool closed = false, Color? color = null)
	{
		for (var i = 0; i + 1 < points.Length; i++)
		{
			Segment(points[i], points[i + 1], color, 0);
		}
		if (closed && points.Length > 2)
		{
			Segment(points[^1], points[0], color, 0);
		}
	}

	/// <summary>Three short lines crossing at a point, as for marking a place.</summary>
	public static void Cross(Vector3 point, float size = 0.25f, Color? color = null)
	{
		var half = size * 0.5f;
		Segment(point - new Vector3(half, 0f, 0f), point + new Vector3(half, 0f, 0f), color, 0);
		Segment(point - new Vector3(0f, half, 0f), point + new Vector3(0f, half, 0f), color, 0);
		Segment(point - new Vector3(0f, 0f, half), point + new Vector3(0f, 0f, half), color, 0);
	}

	/// <summary>A circle around center, in the plane across normal.</summary>
	public static void Circle(Vector3 center, Vector3 normal, float radius, Color? color = null)
	{
		var (u, v) = Across(normal);
		Ring(center, u, v, radius, 0f, MathF.Tau, color);
	}

	/// <summary>
	/// Part of a circle around center, in the plane across normal: starting from the direction from, and turning
	/// degrees around normal, as a rotation by that many degrees would.
	/// </summary>
	public static void Arc(Vector3 center, Vector3 normal, Vector3 from, float degrees, float radius, Color? color = null)
	{
		var axis = Vector3.Normalize(normal);
		var start = from - axis * Vector3.Dot(from, axis);
		if (start.LengthSquared() < 1e-12f)
		{
			start = Across(axis).U;
		}
		start = Vector3.Normalize(start);
		Ring(center, start, Vector3.Cross(axis, start), radius, 0f, degrees * MathF.PI / 180f, color);
	}

	/// <summary>A box's twelve edges, size across on each of its own axes, turned by rotation.</summary>
	public static void WireBox(Vector3 center, Vector3 size, Quaternion? rotation = null, Color? color = null)
	{
		var turn = rotation ?? Quaternion.Identity;
		var half = size * 0.5f;
		Span<Vector3> corners = stackalloc Vector3[8];
		for (var i = 0; i < 8; i++)
		{
			var sign = new Vector3((i & 1) == 0 ? -1f : 1f, (i & 2) == 0 ? -1f : 1f, (i & 4) == 0 ? -1f : 1f);
			corners[i] = center + Vector3.Transform(half * sign, turn);
		}
		// Corners that differ in one bit share an edge.
		for (var a = 0; a < 8; a++)
		{
			foreach (var bit in (ReadOnlySpan<int>)[1, 2, 4])
			{
				if ((a & bit) == 0)
				{
					Segment(corners[a], corners[a | bit], color, 0);
				}
			}
		}
	}

	/// <summary>A sphere, as a circle around each axis.</summary>
	public static void WireSphere(Vector3 center, float radius, Color? color = null)
	{
		Ring(center, Vector3.UnitX, Vector3.UnitY, radius, 0f, MathF.Tau, color);
		Ring(center, Vector3.UnitY, Vector3.UnitZ, radius, 0f, MathF.Tau, color);
		Ring(center, Vector3.UnitZ, Vector3.UnitX, radius, 0f, MathF.Tau, color);
	}

	/// <summary>A capsule from start to end, radius around the line between them: a circle at each end, four sides, and rounded caps.</summary>
	public static void WireCapsule(Vector3 start, Vector3 end, float radius, Color? color = null)
	{
		var along = end - start;
		if (along.LengthSquared() < 1e-12f)
		{
			WireSphere(start, radius, color);
			return;
		}
		var axis = Vector3.Normalize(along);
		var (u, v) = Across(axis);
		Ring(start, u, v, radius, 0f, MathF.Tau, color);
		Ring(end, u, v, radius, 0f, MathF.Tau, color);
		foreach (var side in (ReadOnlySpan<Vector3>)[u, -u, v, -v])
		{
			Segment(start + side * radius, end + side * radius, color, 0);
		}
		foreach (var side in (ReadOnlySpan<Vector3>)[u, v])
		{
			Ring(end, side, axis, radius, 0f, MathF.PI, color);
			Ring(start, side, -axis, radius, 0f, MathF.PI, color);
		}
	}

	/// <summary>A cylinder from start to end, radius around the line between them: a circle at each end, and four sides.</summary>
	public static void WireCylinder(Vector3 start, Vector3 end, float radius, Color? color = null)
	{
		var along = end - start;
		var (u, v) = Across(along.LengthSquared() < 1e-12f ? Vector3.UnitY : along);
		Ring(start, u, v, radius, 0f, MathF.Tau, color);
		Ring(end, u, v, radius, 0f, MathF.Tau, color);
		foreach (var side in (ReadOnlySpan<Vector3>)[u, -u, v, -v])
		{
			Segment(start + side * radius, end + side * radius, color, 0);
		}
	}

	/// <summary>
	/// A cone from its tip at apex along direction, as far as direction is long, opening degrees from its middle to
	/// its side, as a spot light's cone.
	/// </summary>
	public static void WireCone(Vector3 apex, Vector3 direction, float degrees, Color? color = null)
	{
		if (direction.LengthSquared() < 1e-12f)
		{
			return;
		}
		var baseCenter = apex + direction;
		var radius = MathF.Tan(Math.Clamp(degrees, 0f, 89f) * MathF.PI / 180f) * direction.Length();
		var (u, v) = Across(direction);
		Ring(baseCenter, u, v, radius, 0f, MathF.Tau, color);
		foreach (var side in (ReadOnlySpan<Vector3>)[u, -u, v, -v])
		{
			Segment(apex, baseCenter + side * radius, color, 0);
		}
	}

	/// <summary>What a camera at position, turned by rotation, sees between near and far: its frustum's twelve edges.</summary>
	public static void Frustum(Vector3 position, Quaternion rotation, float fovDegrees, float aspect, float near, float far, Color? color = null)
	{
		var forward = Vector3.Transform(Vector3.UnitZ, rotation);
		var right = Vector3.Transform(Vector3.UnitX, rotation);
		var up = Vector3.Transform(Vector3.UnitY, rotation);
		var tan = MathF.Tan(fovDegrees * MathF.PI / 360f);
		Span<Vector3> corners = stackalloc Vector3[8];
		for (var i = 0; i < 8; i++)
		{
			var distance = i < 4 ? near : far;
			var (x, y) = ((i & 1) == 0 ? -1f : 1f, (i & 2) == 0 ? -1f : 1f);
			corners[i] = position + forward * distance + right * (x * tan * aspect * distance) + up * (y * tan * distance);
		}
		for (var plane = 0; plane < 8; plane += 4)
		{
			Segment(corners[plane], corners[plane + 1], color, 0);
			Segment(corners[plane + 1], corners[plane + 3], color, 0);
			Segment(corners[plane + 3], corners[plane + 2], color, 0);
			Segment(corners[plane + 2], corners[plane], color, 0);
		}
		for (var i = 0; i < 4; i++)
		{
			Segment(corners[i], corners[i + 4], color, 0);
		}
	}

	/// <summary>A smooth curve from start to end, pulled towards the two handles between, as a path or a cable.</summary>
	public static void Bezier(Vector3 start, Vector3 startHandle, Vector3 endHandle, Vector3 end, Color? color = null)
	{
		var last = start;
		for (var i = 1; i <= Segments; i++)
		{
			var t = i / (float)Segments;
			var s = 1f - t;
			var point = s * s * s * start + 3f * s * s * t * startHandle + 3f * s * t * t * endHandle + t * t * t * end;
			Segment(last, point, color, 0);
			last = point;
		}
	}

	/// <summary>A square grid of cells around center, size across, flat on the ground unless rotation turns it.</summary>
	public static void Grid(Vector3 center, float size, int cells, Quaternion? rotation = null, Color? color = null)
	{
		var turn = rotation ?? Quaternion.Identity;
		var x = Vector3.Transform(Vector3.UnitX, turn) * size;
		var z = Vector3.Transform(Vector3.UnitZ, turn) * size;
		var corner = center - (x + z) * 0.5f;
		var count = Math.Max(1, cells);
		for (var i = 0; i <= count; i++)
		{
			var t = i / (float)count;
			Segment(corner + x * t, corner + x * t + z, color, 0);
			Segment(corner + z * t, corner + z * t + x, color, 0);
		}
	}

	/// <summary>Text at a point, facing the camera at a fixed size, in the scope's colour unless given one.</summary>
	public static void Label(Vector3 position, string text, Color? color = null)
	{
		ref var now = ref State;
		var at = Vector3.Transform(position, now.Matrix);
		var shade = color ?? now.Color;
		if (Sink != null || !Native.IsConnected)
		{
			return;
		}

		var bytes = Encoding.UTF8.GetBytes(text);
		fixed (byte* written = bytes)
		{
			Native.Api->DebugText(&at, written, bytes.Length, &shade, now.Duration);
		}
	}

	/// <summary>
	/// A line of text in the view's top-right corner, under the lines drawn before it, in the scope's colour unless
	/// given one: a value to watch while playing, as Draw.ScreenText($"speed {speed:0.0}").
	/// </summary>
	public static void ScreenText(string text, Color? color = null)
	{
		ref var now = ref State;
		var shade = color ?? now.Color;
		if (Sink != null || !Native.IsConnected)
		{
			return;
		}

		var bytes = Encoding.UTF8.GetBytes(text);
		fixed (byte* written = bytes)
		{
			Native.Api->DebugScreenText(written, bytes.Length, &shade, now.Duration);
		}
	}

	internal static void Restore(DrawState before)
	{
		State = before;
	}

	/// <summary>Two directions across a normal, at right angles to it and to each other.</summary>
	private static (Vector3 U, Vector3 V) Across(Vector3 normal)
	{
		var axis = Vector3.Normalize(normal);
		var helper = MathF.Abs(axis.Y) < 0.9f ? Vector3.UnitY : Vector3.UnitX;
		var u = Vector3.Normalize(Vector3.Cross(helper, axis));
		return (u, Vector3.Cross(axis, u));
	}

	/// <summary>Part of a circle around center through u at angle 0 and v at a quarter turn, from one angle to another.</summary>
	private static void Ring(Vector3 center, Vector3 u, Vector3 v, float radius, float from, float to, Color? color)
	{
		var steps = Math.Max(2, (int)MathF.Ceiling(Segments * MathF.Abs(to - from) / MathF.Tau));
		var last = center + u * (MathF.Cos(from) * radius) + v * (MathF.Sin(from) * radius);
		for (var i = 1; i <= steps; i++)
		{
			var angle = from + (to - from) * i / steps;
			var point = center + u * (MathF.Cos(angle) * radius) + v * (MathF.Sin(angle) * radius);
			Segment(last, point, color, 0);
			last = point;
		}
	}

	/// <summary>One line, placed by the scope's matrix and drawn as the scopes say.</summary>
	private static void Segment(Vector3 from, Vector3 to, Color? color, int flags)
	{
		ref var now = ref State;
		var a = Vector3.Transform(from, now.Matrix);
		var b = Vector3.Transform(to, now.Matrix);
		var shade = color ?? now.Color;
		flags |= (now.OnTop ? OnTopFlag : 0) | (now.InGame ? InGameFlag : 0) | (now.Gizmo ? GizmoFlag : 0);
		flags |= now.WidthInMetres
			? MetresFlag | (int)MathF.Round(now.Width * 1000f) << WidthShift
			: (int)MathF.Round(Math.Min(now.Width, 4095f) * 16f) << WidthShift;
		if (now.Gizmo)
		{
			GizmoLines++;
		}
		if (Sink != null)
		{
			Sink(a, b, shade, now.Duration, flags);
			return;
		}
		if (Native.IsConnected)
		{
			Native.Api->DebugLine(&a, &b, &shade, now.Duration, flags);
		}
	}
}
