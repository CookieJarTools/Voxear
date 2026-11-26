using System;
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
		Check.True(!Voxels.Print(VoxelPrint.Load("Prints/Crate.voxprint"), Vector3.Zero, 90f), "no volume to print into");
		Check.True(Voxels.Box(Vector3.Zero, Vector3.One, VoxelMaterial.Cobbles, turn: 20f, clear: 2f) == 0, "no volume to pave");
		Check.True(Voxels.CarveBox(Vector3.Zero, Vector3.One) == 0 && Voxels.Slab(Vector3.Zero, 1f, 1f, VoxelMaterial.Ashlar) == 0, "nor to dig");
		Check.True(Voxels.Ramp(Vector3.Zero, Vector3.One, VoxelMaterial.Dirt) == 0, "no bank");
		Vector2[] square = [new(0f, 0f), new(4f, 0f), new(4f, 4f), new(0f, 4f)];
		Check.True(Voxels.Area(square, 1f, 1.5f, VoxelMaterial.Flagstones, tint: Color.Red) == 0, "no street");
		Check.True(Voxels.Cover(square, 0.25f, VoxelMaterial.Bricks) == 0 && Voxels.CarveArea(square, 0f, 1f) == 0, "no cover");
		var house = VoxelPrint.Load("Prints/House.voxprint");
		Check.True(!Voxels.Print(house, Vector3.Zero, Vector3.Zero, fit: true, footing: VoxelMaterial.Ashlar), "no house");
		Check.True(Voxels.Dig(Vector3.Zero) == 0 && Voxels.Place(Vector3.One, VoxelMaterial.Planks, Color.Red) == 0, "no voxel set");
		Check.True(Voxels.Restore([Vector3.Zero, Vector3.One]) == 0 && Voxels.Dig(ReadOnlySpan<Vector3>.Empty) == 0, "nor let go");
		Check.True(!Voxels.WaterAt(Vector3.Zero, out var water) && water == default, "no river to find");
		Check.True(Voxels.Cut(Vector3.Zero, 1f) == default && Voxels.Cut([Vector3.Zero, Vector3.One]) == default, "no grass to cut");
		Check.True(Voxels.Slice(Vector3.Zero, Vector3.UnitY, 0.8f, fall: Vector3.UnitX, rest: VoxelPieceRest.Stamp).Felled == 0, "no tree to fell");
		Check.True(Voxels.Cut(Vector3.Zero, 1f, VoxelPieces.Remove).All.IsEmpty, "no piece to take away");
		Check.True(!Voxels.Piece(default, out var piece) && piece == default && !Voxels.StampPiece(default) && !Voxels.RemovePiece(default), "no felled piece");
		Check.True(!Voxels.Raycast(Vector3.Zero, -Vector3.UnitY, out _, 10f, VoxelRayMeets.Cover), "no cover to meet");
		Voxels.Sea = true;
		Check.True(!Voxels.Sea && float.IsNaN(Voxels.SeaLevel), "no sea");
		Voxels.WaterLook = VoxelWaterLook.Smooth;
		Check.True(Voxels.WaterLook == VoxelWaterLook.Voxels, "no volume to draw smooth water");
	}

	[Test]
	private static void APrintIsNamedByItsReference()
	{
		var crate = VoxelPrint.Load("Prints/Crate.voxprint");
		Check.True(crate.IsSet && crate == VoxelPrint.Load("Prints/Crate.voxprint"), "the same reference, the same print");
		Check.True(!VoxelPrint.None.IsSet && !Voxels.Print(VoxelPrint.None, Vector3.Zero), "none prints nothing");
	}

	[Test]
	private static void GroundsAreNumberedAsTheEngineNumbersThem()
	{
		// VoxelMaterial::ALL in voxear_render: Dirt, Grass, Rock, Sand, Snow, Gravel, then the pavings.
		Check.True((int)VoxelMaterial.Dirt == 0 && (int)VoxelMaterial.Rock == 2 && (int)VoxelMaterial.Gravel == 5, "in order");
		Check.True((int)VoxelMaterial.Cobbles == 6 && (int)VoxelMaterial.Ashlar == 10, "the pavings after the grounds");
	}
}
