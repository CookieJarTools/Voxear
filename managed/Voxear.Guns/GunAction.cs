namespace Voxear;

/// <summary>What a Gun Holder does with its gun, by hand, with no animation clips.</summary>
public enum GunAction
{
	/// <summary>Fills the gun as its kind is filled: a magazine swapped, a stripper clip pressed, shells put in.</summary>
	Reload,

	/// <summary>Works the bolt, slide or pump by hand.</summary>
	Cycle,

	/// <summary>Looks at how much is in the magazine, clip, belt or cylinder.</summary>
	CheckMagazine,

	/// <summary>Opens the action a little to see whether a round is chambered.</summary>
	CheckChamber,
}
