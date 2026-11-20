namespace Voxear;

/// <summary>Where a Post Volume's settings apply.</summary>
public enum PostVolumeShape
{
	/// <summary>Everywhere, as a level-wide layer over the level's settings.</summary>
	Global,

	/// <summary>Inside its Obj's unit cube, as its Transform sizes it.</summary>
	Box,

	/// <summary>Inside the sphere round its Obj's unit cube, as wide as its largest scale.</summary>
	Sphere,
}
