using System.Numerics;

namespace Voxear.Tests;

/// <summary>The Dry Area API with no engine behind it: no Obj has one, every value reads as none, nothing throws.</summary>
internal static class DryAreaTests
{
	[Test]
	private static void NoObjHasADryAreaWithoutTheEngine()
	{
		Check.True(Obj.None.DryArea is null, "none to find");
		Obj.None.DryArea?.Bilge = 1f;
		var hold = Obj.None.AddDryArea();
		Check.True(!hold.IsValid && hold == new DryArea(Obj.None), "nothing to add it to");
		Check.True(hold != Obj.None.DryArea, "and no Dry Area to equal");
		hold.Bilge = 1.5f;
		hold.BilgeFill = 0.5f;
		hold.BilgeEnabled = false;
		hold.Shape = DryShape.Mesh;
		hold.Size = new Vector3(3f, 2f, 8f);
		Check.True(hold.Bilge == 0f && hold.BilgeFill == 0f && !hold.BilgeEnabled, "nothing to read");
		Check.True(hold.InsideHeight == 0f, "nothing to read");
		Check.True(hold.Shape == DryShape.Box && hold.Size == Vector3.Zero, "nothing to read");
		hold.Remove();
		Check.True(hold.ToString().Contains("Dry Area"), hold.ToString());
	}
}
