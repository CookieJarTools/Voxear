namespace Voxear;

/// <summary>What a LOD Group's levels are measured by.</summary>
public enum LodMeasure
{
	/// <summary>How much of the screen's height the group's Objs cover.</summary>
	ScreenSize,

	/// <summary>How far the camera is from the group's Obj, in metres.</summary>
	Distance,
}
