using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// Lighting settings of the project, as a field holds them: a <c>.lighting</c> asset with a sky, how the sun looks
/// and post-processing, picked in the Inspector or dragged there from the Project panel. Give it to Lighting.Apply
/// to light the level with it. It names the asset by its ID, so renaming or moving the file keeps the field pointing
/// at it.
/// <code>
/// public LightingSettings Dusk;
/// ...
/// Lighting.Apply(Dusk);
/// </code>
/// </summary>
public readonly struct LightingSettings : IEquatable<LightingSettings>, IAsset<LightingSettings>
{
	public static readonly LightingSettings None = default;

	/// <summary>The built-in Golden Hour lighting: a low, warm sun in dusty air.</summary>
	public static readonly LightingSettings GoldenHour = new("builtin:golden_hour");

	/// <summary>The built-in Storybook lighting: a warm sun through a canopy into a cool forest.</summary>
	public static readonly LightingSettings Storybook = new("builtin:storybook");

	/// <summary>The built-in Soft Overcast lighting: hazy, bright and calm.</summary>
	public static readonly LightingSettings SoftOvercast = new("builtin:soft_overcast");

	/// <summary>The built-in Overgrown Dam lighting: humid green-gold haze over lush ruins.</summary>
	public static readonly LightingSettings OvergrownDam = new("builtin:overgrown_dam");

	/// <summary>The built-in Sand-Buried City lighting: a harsh sun in thick orange dust.</summary>
	public static readonly LightingSettings SandBuriedCity = new("builtin:sand_buried_city");

	/// <summary>The built-in Rusted Spaceport lighting: flat, cool industrial overcast.</summary>
	public static readonly LightingSettings RustedSpaceport = new("builtin:rusted_spaceport");

	/// <summary>The built-in Cold Mountain Pass lighting: a crisp sun over blue valley haze.</summary>
	public static readonly LightingSettings ColdMountainPass = new("builtin:cold_mountain_pass");

	/// <summary>The built-in Snowbound Peak lighting: white, cold and bright.</summary>
	public static readonly LightingSettings SnowboundPeak = new("builtin:snowbound_peak");

	/// <summary>The built-in Night Raid lighting: moonlight and lamps in thin mist.</summary>
	public static readonly LightingSettings NightRaid = new("builtin:night_raid");

	/// <summary>The built-in Electrical Storm lighting: a dark green-grey sky and heavy air.</summary>
	public static readonly LightingSettings ElectricalStorm = new("builtin:electrical_storm");

	/// <summary>The built-in Candlelit Castle lighting: warm window light and dust in dark halls.</summary>
	public static readonly LightingSettings CandlelitCastle = new("builtin:candlelit_castle");

	/// <summary>The built-in Dark Forest lighting: teal gloom cut by light shafts.</summary>
	public static readonly LightingSettings DarkForest = new("builtin:dark_forest");

	/// <summary>The built-in Snowy Village lighting: soft winter light, with warm windows glowing.</summary>
	public static readonly LightingSettings SnowyVillage = new("builtin:snowy_village");

	/// <summary>The built-in Highland Summer lighting: bright, rich and painterly.</summary>
	public static readonly LightingSettings HighlandSummer = new("builtin:highland_summer");

	/// <summary>The built-in Enchanted Night lighting: a violet moonlit sky full of stars.</summary>
	public static readonly LightingSettings EnchantedNight = new("builtin:enchanted_night");

	private readonly string? reference;

	private LightingSettings(string reference)
	{
		this.reference = reference;
	}

	/// <summary>Whether it names lighting settings.</summary>
	public bool IsSet => !string.IsNullOrEmpty(reference);

	/// <summary>The asset's GUID or project path, as the scene or code gave it.</summary>
	public string Reference => reference ?? string.Empty;

	string? IAsset<LightingSettings>.AssetGuid => IsSet ? NativeSaves.Key(NativeSaves.KeyReference, 0, Reference) : null;

	/// <summary>
	/// The lighting settings at a project path or GUID, as "Lighting/Dusk.lighting". Nothing is read until they are
	/// applied.
	/// </summary>
	public static LightingSettings Load(string path)
	{
		return new LightingSettings(path);
	}

	public static bool operator ==(LightingSettings left, LightingSettings right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(LightingSettings left, LightingSettings right)
	{
		return !left.Equals(right);
	}

	public bool Equals(LightingSettings other)
	{
		return Reference == other.Reference;
	}

	public override bool Equals(object? other)
	{
		return other is LightingSettings settings && Equals(settings);
	}

	public override int GetHashCode()
	{
		return Reference.GetHashCode();
	}

	public override string ToString()
	{
		return IsSet ? Reference : "no lighting settings";
	}
}
