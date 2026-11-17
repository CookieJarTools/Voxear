using System;
using System.Collections.Generic;
using System.Numerics;
using System.Runtime.CompilerServices;

namespace Voxear;

/// <summary>
/// An Obj moving into another dimension, from <c>obj.MoveTo</c>: the dimension made first where it is not, the Obj
/// moved, with everything under it and keeping its speed, then the ground round where it stands made. Await it to
/// carry on once it stands there; the await gives whether it went.
/// <code>
/// if (await player.MoveTo(caves, entrance))
/// {
///     ShowTitle("The Caves");
/// }
/// </code>
/// </summary>
public sealed class DimensionMove : INotifyCompletion
{
	/// <summary>How far round where it stands the ground is made before the move is over, in metres.</summary>
	private const float Round = 16f;

	private readonly List<Action> continuations = [];
	private readonly Obj obj;
	private readonly DimensionMake? making;
	private readonly Vector3 position;
	private DimensionInstance into;
	private bool moved;

	internal DimensionMove(Obj obj, DimensionMake making, Vector3 position)
	{
		(this.obj, this.making, this.position) = (obj, making, position);
		Start();
	}

	internal DimensionMove(Obj obj, DimensionInstance into, Vector3 position)
	{
		(this.obj, this.into, this.position) = (obj, into, position);
		Start();
	}

	/// <summary>Whether it is over: moved and the ground stands round it, or it could not go.</summary>
	public bool IsDone { get; private set; }

	/// <summary>Whether the Obj went.</summary>
	public bool Went { get; private set; }

	/// <summary>The dimension it went into.</summary>
	public DimensionInstance Dimension => into;

	public bool IsCompleted => IsDone;

	public DimensionMove GetAwaiter()
	{
		return this;
	}

	public void OnCompleted(Action continuation)
	{
		if (IsDone)
		{
			continuation();
			return;
		}
		continuations.Add(continuation);
	}

	public bool GetResult()
	{
		return Went;
	}

	private void Start()
	{
		if (!Poll())
		{
			Dimensions.Wait(Poll);
		}
	}

	/// <summary>Moves on as far as it can, and returns true once it is over.</summary>
	private bool Poll()
	{
		if (!obj.IsValid)
		{
			return Finish(false);
		}
		if (making is { IsDone: false })
		{
			return false;
		}
		if (making is { IsMade: false })
		{
			return Finish(false);
		}
		if (!moved)
		{
			into = making?.Dimension ?? into;
			if (!Interop.NativeDimensions.Move(obj, into.Handle, position))
			{
				Log.Error($"{obj.Name} could not be moved into {into}: it is not made.");
				return Finish(false);
			}
			moved = true;
		}
		return into.IsReadyAround(position, Round) && Finish(true);
	}

	private bool Finish(bool went)
	{
		(Went, IsDone) = (went, true);
		var waiting = continuations.ToArray();
		continuations.Clear();
		foreach (var continuation in waiting)
		{
			continuation();
		}
		return true;
	}
}
