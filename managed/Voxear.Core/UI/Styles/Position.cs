namespace Voxear;

/// <summary>How an element is placed: in line with the others, or exactly, within the box it is in.</summary>
public enum Position
{
	/// <summary>In line with the elements around it; Left and Top move it from there.</summary>
	Relative,

	/// <summary>Out of the line, placed by Left, Top, Right and Bottom within the box it is in.</summary>
	Absolute,
}
