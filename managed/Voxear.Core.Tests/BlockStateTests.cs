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

	[Test]
	private static void ALogKeepsHowThickItIsAsTheEngineDoes()
	{
		// Lying east, six sixteenths across: the top bit set, its thickness over its facing.
		var branch = new BlockState(BlockFacing.East, Size: 6);
		Check.True(branch.ToByte() == (1 | (6 << 3) | 0x80), $"the engine's byte, not {branch.ToByte()}");
		Check.True(BlockState.FromByte(branch.ToByte()) == branch, "a log's state comes back from its byte");
		Check.True(branch.Across == 6 && new BlockState(BlockFacing.Up).Across == 16, "whole is sixteen across");
		Check.True(BlockState.FromByte(16).Open && BlockState.FromByte(16).Size == 0, "a door's byte is no log's");
	}

	[Test]
	private static void ABlockSaysWhichBlockItShowsAsWhileCovered()
	{
		// As the engine says a block to scripts: grass, which shows as dirt under a block.
		const string said = """
			{ "name": "Grass", "color": [0.2, 0.6, 0.1], "liquid": "none", "hardness": 0.6, "unbreakable": false,
			  "tool": null, "tier": 0, "data": null, "drops": [], "sounds": { "break": null, "place": null, "step": null },
			  "covered": "test:covered-dirt" }
			""";
		using var json = System.Text.Json.JsonDocument.Parse(said);
		var grass = BlockInfos.Make("test:covered-grass", new BlockInfo(json.RootElement));
		var dirt = BlockInfos.Make("test:covered-dirt", new BlockInfo("Dirt"));
		Check.True(grass.Covered == dirt, $"grass shows as dirt, not {grass.Covered}");
		Check.True(dirt.Covered == Block.None, "dirt shows as itself always");
	}
}
