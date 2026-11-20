using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>What the renderer keeps room for, as Project Settings ▸ Quality has it: fixed once the game has started.</summary>
public sealed class QualityBudgets
{
	/// <summary>How many particles every system together may have at once.</summary>
	public int Particles { get; private set; } = 1_000_000;

	/// <summary>What share of the room every mesh shares, in percent, worlds of blocks' lighting may take.</summary>
	public int BlockLighting { get; private set; } = 75;

	/// <summary>How many vertices of posed models the renderer keeps room for.</summary>
	public int PosedVertices { get; private set; } = 1024 * 1024;

	internal static QualityBudgets Read(JsonObject from)
	{
		var settings = new QualityBudgets();
		settings.Particles = SettingsJson.Read(from, "particles", settings.Particles);
		settings.BlockLighting = SettingsJson.Read(from, "block_lighting", settings.BlockLighting);
		settings.PosedVertices = SettingsJson.Read(from, "posed_vertices", settings.PosedVertices);
		return settings;
	}
}
