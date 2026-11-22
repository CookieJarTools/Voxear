using System.Collections.Generic;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A world's own data for each area tag, where no World Area there carries the tag: for a biome first, then for
/// anywhere. On the world's Obj. Get it with Obj.AreaDefaults, or give an Obj one with Obj.AddAreaDefaults.
/// <code>
/// Obj.AddAreaDefaults().Tags = [new AreaEntry(Music, Wilds), new AreaEntry(Music, DesertSong, Desert)];
/// </code>
/// </summary>
public sealed class AreaDefaults : WholeComponent
{
	internal AreaDefaults(Obj obj)
		: base(obj, NativeWhole.Whole, NativeWhole.AreaDefaults)
	{
	}

	/// <summary>Each tag's data, for a biome or, with Biome.None, for anywhere.</summary>
	public IReadOnlyList<AreaEntry> Tags
	{
		get => AreaJson.Entries(Settings());
		set => Change("tags", AreaJson.Of(value));
	}

	internal static AreaDefaults? Of(Obj obj)
	{
		var defaults = new AreaDefaults(obj);
		return defaults.IsValid ? defaults : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s Area Defaults";
	}
}
