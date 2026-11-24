namespace Voxear;

/// <summary>How a physical Camera's sensor is fitted to a picture of another shape.</summary>
public enum GateFit
{
	/// <summary>The sensor's height fills the picture's height.</summary>
	Vertical,

	/// <summary>The sensor's width fills the picture's width.</summary>
	Horizontal,

	/// <summary>The whole sensor shows, the picture seeing more one way.</summary>
	Fill,

	/// <summary>The picture is filled, part of the sensor cut off one way.</summary>
	Overscan,

	/// <summary>The picture takes the sensor's own shape, stretched.</summary>
	None,
}
