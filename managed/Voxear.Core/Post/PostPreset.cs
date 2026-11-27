namespace Voxear;

/// <summary>
/// A finished set of post-processing settings to start from: the three presets, and the post-processing of each built-in
/// lighting look.
/// </summary>
public enum PostPreset
{
	/// <summary>The Voxear Look: ambient occlusion, TAA, soft bloom, eye adaptation and a little punch.</summary>
	Voxear,

	/// <summary>Only what keeps the picture right, with no stylistic effects.</summary>
	Neutral,

	/// <summary>A filmic camera: stronger bloom and vignette, lens fringing and motion blur.</summary>
	Cinematic,

	/// <summary>Golden Hour's: warm wide bloom and lifted shade.</summary>
	GoldenHour,

	/// <summary>Storybook's: gold bloom and a teal-and-gold grade.</summary>
	Storybook,

	/// <summary>Soft Overcast's: calm exposure and a soft grade.</summary>
	SoftOvercast,

	OvergrownDam,
	SandBuriedCity,
	RustedSpaceport,
	ColdMountainPass,
	SnowboundPeak,
	NightRaid,
	ElectricalStorm,
	CandlelitCastle,
	DarkForest,
	SnowyVillage,
	HighlandSummer,
	EnchantedNight,

	/// <summary>Abyss's: dim warm smoke, heavy vignette, colour fringes, grain, and a view that breathes and wavers.</summary>
	Abyss,
}
