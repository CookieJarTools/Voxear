namespace Voxear;

/// <summary>Where a Wind Zone blows.</summary>
public enum WindShape
{
	/// <summary>Over the whole level: the first one sways the plants, and can blow the sea's wind.</summary>
	Global,

	/// <summary>Inside a ball turned with its Obj, where the sea's short waves are as rough as its strength says.</summary>
	Sphere,

	/// <summary>Inside a box turned with its Obj, where the sea's short waves are as rough as its strength says.</summary>
	Box,
}
