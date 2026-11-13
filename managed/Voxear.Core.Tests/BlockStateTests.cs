namespace Voxear.Tests;

internal static class BlockStateTests
{
	[Test]
	private static void AStateIsOneByteAsTheEngineKeepsIt()
	{
		var door = new BlockState(BlockFacing.West, BlockHalf.Upper, Open: true, Hinge: DoorHinge.Right);
		Check.True(BlockState.FromByte(door.ToByte()) == door, "a door's state comes back from its byte");
		// The engine's byte: facing in three bits, then half, open, on and hinge.
		Check.True(door.ToByte() == (3 | 8 | 16 | 64), $"the engine's byte, not {door.ToByte()}");
		Check.True(BlockState.FromByte(5).Facing == BlockFacing.Down, "5 faces down");
		Check.True(BlockState.FromByte(32) == default(BlockState) with { On = true }, "32 is on");
	}
}
