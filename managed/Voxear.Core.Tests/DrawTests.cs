using System;
using System.Collections.Generic;
using System.Numerics;

namespace Voxear.Tests;

internal static class DrawTests
{
	[Test]
	private static void ScopesSetHowLinesDrawAndPutItBack()
	{
		var lines = Record(() =>
		{
			using (Draw.Color(Color.Red))
			using (Draw.Width(3f))
			using (Draw.OnTop())
			{
				Draw.Line(Vector3.Zero, Vector3.UnitX);
				using (Draw.Duration(2f))
				using (Draw.InGame())
				{
					Draw.Line(Vector3.Zero, Vector3.UnitY, Color.Blue);
				}
			}
			Draw.Line(Vector3.Zero, Vector3.UnitZ);
		});
		Check.True(lines.Count == 3, $"three lines, not {lines.Count}");
		var (_, _, red, redDuration, redFlags) = lines[0];
		Check.True(red.R == 1f && red.G == 0f, "the scope's colour");
		Check.True((redFlags & 1) == 1 && (redFlags >> 8) == 48, $"on top, and 3 pixels in sixteenths: {redFlags}");
		Check.Near(0f, redDuration);
		var (_, _, blue, blueDuration, blueFlags) = lines[1];
		Check.True(blue.B == 1f && blue.R == 0f, "a colour given to the shape wins over the scope's");
		Check.True((blueFlags & 4) == 4, "in game inside InGame");
		Check.Near(2f, blueDuration);
		var (_, _, after, _, afterFlags) = lines[2];
		Check.True(after.R == 1f && after.G == 1f && after.B == 1f && afterFlags == 0, "white and plain again after the scopes");
	}

	[Test]
	private static void AWidthInMetresIsSentInMillimetres()
	{
		var lines = Record(() =>
		{
			using (Draw.WidthInMetres(0.25f))
			{
				Draw.Line(Vector3.Zero, Vector3.UnitX);
				using (Draw.Width(2f))
				{
					Draw.Line(Vector3.Zero, Vector3.UnitY);
				}
			}
		});
		Check.True((lines[0].Flags & 16) == 16 && (lines[0].Flags >> 8) == 250, $"in metres, 250 mm: {lines[0].Flags}");
		Check.True((lines[1].Flags & 16) == 0 && (lines[1].Flags >> 8) == 32, $"pixels again inside Width: {lines[1].Flags}");
	}

	[Test]
	private static void SolidShapesAreTrianglesWoundOutwards()
	{
		var drawn = new List<(Vector3[] Corners, int Flags)>();
		Draw.SolidSink = (corners, flags) => drawn.Add((corners, flags));
		try
		{
			using (Draw.OnTop())
			{
				Draw.SolidBox(Vector3.One, new Vector3(2f));
			}
			Draw.SolidSphere(Vector3.Zero, 1f);
			Draw.Triangle(Vector3.Zero, Vector3.UnitX, Vector3.UnitY);
		}
		finally
		{
			Draw.SolidSink = null;
		}
		var (box, boxFlags) = drawn[0];
		Check.True(box.Length == 36 && boxFlags == 1, $"twelve triangles, on top: {box.Length}, {boxFlags}");
		Check.True(Outwards(box, Vector3.One), "the box's faces wound outwards");
		Check.True(Outwards(drawn[1].Corners, Vector3.Zero), "the sphere's too");
		Check.True(drawn[2].Corners.Length == 6, "a triangle shows from both sides");
	}

	private static bool Outwards(Vector3[] corners, Vector3 middle)
	{
		for (var i = 0; i < corners.Length; i += 3)
		{
			var (a, b, c) = (corners[i], corners[i + 1], corners[i + 2]);
			if (Vector3.Dot(Vector3.Cross(b - a, c - a), (a + b + c) / 3f - middle) < -1e-4f)
			{
				return false;
			}
		}
		return true;
	}

	[Test]
	private static void MatricesPlaceWhatIsDrawnInsideThem()
	{
		var lines = Record(() =>
		{
			using (Draw.Matrix(Matrix4x4.CreateTranslation(1f, 2f, 3f)))
			using (Draw.Matrix(Matrix4x4.CreateScale(2f)))
			{
				Draw.Line(Vector3.Zero, Vector3.UnitX);
			}
		});
		Check.Near(new Vector3(1f, 2f, 3f), lines[0].From);
		Check.Near(new Vector3(3f, 2f, 3f), lines[0].To, 1e-5f);
	}

	[Test]
	private static void ShapesAreTheirLines()
	{
		Check.True(Record(() => Draw.WireBox(Vector3.Zero, Vector3.One)).Count == 12, "a box's twelve edges");
		Check.True(Record(() => Draw.WireSphere(Vector3.Zero, 1f)).Count == 96, "three circles of 32");
		Check.True(Record(() => Draw.Frustum(Vector3.Zero, Quaternion.Identity, 60f, 1.5f, 0.1f, 10f)).Count == 12, "a frustum's twelve edges");
		Check.True(Record(() => Draw.Grid(Vector3.Zero, 10f, 4)).Count == 10, "five lines each way for four cells");
		Check.True(Record(() => Draw.Polyline([Vector3.Zero, Vector3.UnitX, Vector3.UnitY], closed: true)).Count == 3, "a closed triangle");

		var circle = Record(() => Draw.Circle(new Vector3(0f, 1f, 0f), Vector3.UnitY, 2f));
		foreach (var (from, _, _, _, _) in circle)
		{
			Check.Near(1f, from.Y, 1e-5f);
			Check.Near(2f, new Vector2(from.X, from.Z).Length(), 1e-4f);
		}

		var arc = Record(() => Draw.Arc(Vector3.Zero, Vector3.UnitY, Vector3.UnitX, 90f, 1f));
		var turned = Vector3.Transform(Vector3.UnitX, Quaternion.CreateFromAxisAngle(Vector3.UnitY, MathF.PI / 2f));
		Check.Near(Vector3.UnitX, arc[0].From, 1e-5f);
		Check.Near(turned, arc[^1].To, 1e-4f);

		var capsule = Record(() => Draw.WireCapsule(Vector3.Zero, Vector3.UnitY * 2f, 0.5f));
		foreach (var (from, _, _, _, _) in capsule)
		{
			var nearest = new Vector3(0f, Math.Clamp(from.Y, 0f, 2f), 0f);
			Check.Near(0.5f, Vector3.Distance(from, nearest), 1e-4f);
		}
	}

	/// <summary>The lines what was given drew, from the start: from, to, colour, duration and flags.</summary>
	private static List<(Vector3 From, Vector3 To, Color Color, float Duration, int Flags)> Record(Action draw)
	{
		var lines = new List<(Vector3, Vector3, Color, float, int)>();
		Draw.Sink = (from, to, color, duration, flags) => lines.Add((from, to, color, duration, flags));
		try
		{
			draw();
		}
		finally
		{
			Draw.Sink = null;
		}
		return lines;
	}
}
