using Voxear.Interop;

namespace Voxear;

/// <summary>
/// One dimension's weather as it is now: the same as <see cref="Weather"/> reads for the dimension the game is seen
/// from. An enclosed dimension, or one whose file turns weather off, is always clear. Got from
/// <see cref="DimensionInstance.Weather"/>.
/// </summary>
public readonly struct DimensionWeather
{
	private readonly DimensionInstance dimension;

	internal DimensionWeather(DimensionInstance dimension)
	{
		this.dimension = dimension;
	}

	/// <summary>Whether weather runs in it at all.</summary>
	public bool HasWeather => NativeDimensions.Weather(dimension.Handle).Y != 0f;

	/// <summary>The kind of weather it is, or is turning to.</summary>
	public WeatherKind Kind => (WeatherKind)(int)NativeDimensions.Weather(dimension.Handle).X;

	/// <summary>What its weather adds this moment.</summary>
	public WeatherAmounts Amounts
	{
		get
		{
			var (first, last) = NativeDimensions.WeatherAmounts(dimension.Handle);
			return new WeatherAmounts
			{
				Cloud = first.X,
				Darkness = first.Y,
				Fog = first.Z,
				Wind = first.W,
				Rain = last.X,
				Snow = last.Y,
				Lightning = last.Z,
				Dust = last.W,
			};
		}
	}
}
