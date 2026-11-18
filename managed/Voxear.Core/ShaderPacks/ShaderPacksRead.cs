using System.Collections.Generic;

namespace Voxear;

/// <summary>What ShaderPacks reads from the engine at once: the packs, the one worn, its options and the folder.</summary>
internal sealed class ShaderPacksRead
{
	public List<ShaderPack> Packs { get; } = [];

	public ShaderPack Worn { get; set; } = ShaderPack.None;

	public ShaderPackStanding Standing { get; set; } = ShaderPackStanding.None;

	public string Error { get; set; } = string.Empty;

	public List<ShaderPackOption> Options { get; } = [];

	public string? Folder { get; set; }
}
