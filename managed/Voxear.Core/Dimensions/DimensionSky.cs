using Voxear.Interop;

namespace Voxear;

/// <summary>One dimension's sky. Got from <see cref="DimensionInstance.Sky"/>.</summary>
public readonly struct DimensionSky
{
	private readonly DimensionInstance dimension;

	internal DimensionSky(DimensionInstance dimension)
	{
		this.dimension = dimension;
	}

	/// <summary>Whether it has a sky. Without one it is enclosed: no sun, no light from the sky, dark beyond its own lights.</summary>
	public bool HasSky => NativeDimensions.Sky(dimension.Handle).X != 0f;

	/// <summary>How much light its sky gives all round, 0 in an enclosed dimension.</summary>
	public float Ambient => NativeDimensions.Sky(dimension.Handle).Y;
}
