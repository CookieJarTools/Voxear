using System.Numerics;
using System.Runtime.InteropServices;
using Voxear.Interop;

namespace Voxear.Tests
{
	internal static class PreviewTests
	{
		[Test]
		private static void OptionsStartAsAnIconWantsThem()
		{
			var data = new PreviewOptions().ToData();
			Check.True(data.Size == 128, $"size {data.Size}");
			Check.True(data.Background == Vector4.Zero, "a clear background");
			Check.Same(Quaternion.CreateFromAxisAngle(Vector3.UnitY, -35f * System.MathF.PI / 180f), data.Rotation);
			Check.Near(1f, data.Zoom);
			Check.Near(30f, data.FieldOfView);
		}

		[Test]
		private static void ASizeOutOfRangeIsKeptToWhatTheEngineDraws()
		{
			Check.True(new PreviewOptions { Size = 4096 }.ToData().Size == 512, "at most 512");
			Check.True(new PreviewOptions { Size = 0 }.ToData().Size == 1, "at least 1");
		}

		[Test]
		private static void TheDataMatchesTheEnginesLayout()
		{
			// voxear_scripting's PreviewOptions and PreviewState are these sizes too.
			Check.True(Marshal.SizeOf<PreviewOptionsData>() == 88, $"options are {Marshal.SizeOf<PreviewOptionsData>()} bytes");
			Check.True(Marshal.SizeOf<PreviewStateData>() == 24, $"state is {Marshal.SizeOf<PreviewStateData>()} bytes");
		}

		[Test]
		private static void WithoutTheEngineThereIsNoPreview()
		{
			Check.True(Preview.Of("Prefabs/Sword.prefab") == null, "nothing to draw it with");
			Check.True(Preview.Of(Prefab.None) == null, "and None is no prefab");
		}
	}
}
