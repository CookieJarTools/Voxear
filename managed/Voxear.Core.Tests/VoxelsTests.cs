using System.Numerics;

namespace Voxear.Tests;

/// <summary>The Voxels API with no engine behind it: no volume to reach, nothing throws.</summary>
internal static class VoxelsTests
{
	[Test]
	private static void NoVolumeIsReachedWithoutTheEngine()
	{
		Check.True(Voxels.Carve(new Vector3(1f, 2f, 3f), 1.5f) == 0, "no volume to carve");
		Check.True(Voxels.Fill(Vector3.Zero, 1f, VoxelMaterial.Rock) == 0, "no volume to fill");
		Check.True(!Voxels.Raycast(Vector3.Zero, -Vector3.UnitY, out var hit) && hit == default, "no ground to meet");
	}

	[Test]
	private static void GroundsAreNumberedAsTheEngineNumbersThem()
	{
		// VoxelMaterial::ALL in voxear_render: Dirt, Grass, Rock, Sand, Snow, Gravel.
		Check.True((int)VoxelMaterial.Dirt == 0 && (int)VoxelMaterial.Rock == 2 && (int)VoxelMaterial.Gravel == 5, "in order");
	}
}
