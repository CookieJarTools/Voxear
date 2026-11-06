using System.Numerics;

namespace Voxear.Tests;

internal static class HandlesTests
{
	[Test]
	private static void AHandleTakesTheDragMeantForItAlone()
	{
		var to = new Vector3(4f, 5f, 6f);
		Handles.BeginFrame(new HandleDrag(7, "Hello.Guard", 1, to, 3f), null);
		Handles.BeginOwner(7, "Hello.Guard");
		var first = Handles.Position(Vector3.One);
		var second = Handles.Position(Vector3.One);
		Handles.BeginOwner(8, "Hello.Guard");
		var elsewhere = Handles.Position(Vector3.One);
		Check.True(first == Vector3.One, "the first handle is not the one held");
		Check.True(second == to, "the second is, and is given back where it was dragged to");
		Check.True(elsewhere == Vector3.One, "another Obj's second handle is not held");
		Check.True(Handles.Shown.Count == 3 && Handles.Shown[1].At == to, "each is shown, the held one where it went");
		Check.True(Handles.Holds(7, "Hello.Guard") && !Handles.Holds(8, "Hello.Guard"), "only the dragged one's behaviour holds it");
		Handles.BeginFrame(null, null);
	}

	[Test]
	private static void ARadiusAndASliderGiveBackTheirNumber()
	{
		Handles.BeginFrame(new HandleDrag(7, "Hello.Guard", 0, Vector3.Zero, 2.5f), null);
		Handles.BeginOwner(7, "Hello.Guard");
		// What they draw goes nowhere: there is no engine to draw it.
		Draw.Sink = (_, _, _, _, _) => { };
		try
		{
			Check.Near(2.5f, Handles.Radius(Vector3.Zero, 1f));
			Check.Near(1f, Handles.Slider(Vector3.Zero, new Vector3(0f, 0f, 2f), 1f));
		}
		finally
		{
			Draw.Sink = null;
		}
		Check.True(Handles.Shown[1].Direction == Vector3.UnitZ, "a slider's direction, made a unit long");
		Handles.BeginFrame(null, null);
	}

	[Test]
	private static void ADiscTurnsAndArrowsMoveAlongTheirAxis()
	{
		Draw.Sink = (_, _, _, _, _) => { };
		try
		{
			// The dot of a ring round Y starts out along +X, and is dragged a quarter turn round to +Z.
			Handles.BeginFrame(new HandleDrag(7, "Hello.Guard", 0, new Vector3(0f, 0f, 2f), 0f), null);
			Handles.BeginOwner(7, "Hello.Guard");
			var turned = Handles.Disc(Vector3.Zero, Quaternion.Identity, Vector3.UnitY, 2f);
			Check.Near(new Vector3(0f, 0f, 1f), Vector3.Transform(Vector3.UnitX, turned), 1e-5f);
			Check.Near(new Vector3(0f, 0f, 1f), Handles.Shown[0].Toward, 1e-5f);
			// The Y arrow, after the point and the X arrow, dragged so its tip is at 3.5 up.
			Handles.BeginFrame(new HandleDrag(7, "Hello.Guard", 2, new Vector3(1f, 3.5f, 1f), 2.5f), null);
			Handles.BeginOwner(7, "Hello.Guard");
			var moved = Handles.Position(new Vector3(1f, 1f, 1f), null, arrows: true);
			Check.Near(new Vector3(1f, 2.5f, 1f), moved, 1e-5f);
			Check.True(Handles.Shown.Count == 4 && Handles.Shown[3].At == moved, "the point and three arrows, the last from where it went");
		}
		finally
		{
			Draw.Sink = null;
			Handles.BeginFrame(null, null);
		}
	}

	[Test]
	private static void AButtonIsTrueWhenClicked()
	{
		Handles.BeginFrame(null, new HandleClick(7, "Hello.Guard", 1));
		Handles.BeginOwner(7, "Hello.Guard");
		var first = Handles.Button(Vector3.Zero);
		var second = Handles.Button(Vector3.UnitY);
		Check.True(!first && second, "only the button clicked");
		Handles.BeginFrame(null, null);
		Handles.BeginOwner(7, "Hello.Guard");
		Handles.Button(Vector3.Zero);
		Check.True(!Handles.Button(Vector3.UnitY), "and only in the frame it was clicked");
	}
}
