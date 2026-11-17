using System;
using System.Numerics;
using System.Runtime.CompilerServices;

namespace Voxear;

/// <summary>
/// Awaits a dimension's ground being made round a place, from <see cref="DimensionInstance.WhenReady"/>. One let go
/// meanwhile carries on at once.
/// </summary>
public readonly struct DimensionReady : INotifyCompletion
{
	private readonly DimensionInstance dimension;
	private readonly Vector3 position;
	private readonly float radius;

	internal DimensionReady(DimensionInstance dimension, Vector3 position, float radius)
	{
		(this.dimension, this.position, this.radius) = (dimension, position, radius);
	}

	public bool IsCompleted => !dimension.IsMade || dimension.IsReadyAround(position, radius);

	public DimensionReady GetAwaiter()
	{
		return this;
	}

	public void OnCompleted(Action continuation)
	{
		var self = this;
		Dimensions.Wait(() =>
		{
			if (!self.IsCompleted)
			{
				return false;
			}
			continuation();
			return true;
		});
	}

	public void GetResult()
	{
	}
}
