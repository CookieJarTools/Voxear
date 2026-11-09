using System;
using System.Numerics;
using Voxear.Interop;
using Voxear.Scripting;

namespace Voxear;

/// <summary>
/// One of a Foliage's plants, by its Obj and its number in the Obj's plants: a tree, a bush, a rock. A ray that meets
/// a plant's trunk says which plant it was (<see cref="RaycastHit.Plant"/>), and <see cref="Plants.Query"/> finds those
/// near a point.
/// <code>
/// if (Physics.Raycast(eye, forward, out var hit, 3f) &amp;&amp; hit.Plant is { } tree &amp;&amp; Input.WasPressed("Use"))
/// {
///     tree.CutDown();
/// }
/// </code>
/// A plant taken away stays away when the game is saved and loaded, and the rest keep their numbers.
/// </summary>
public readonly unsafe struct Plant : IEquatable<Plant>
{
	public Plant(Obj obj, int number)
	{
		Obj = obj;
		Number = number;
	}

	/// <summary>The Obj whose Foliage it is.</summary>
	public Obj Obj { get; }

	/// <summary>Its place in its Obj's plants, which stays its own while the game runs.</summary>
	public int Number { get; }

	/// <summary>Whether it stands: not cut down or taken away, and its Obj still there.</summary>
	public bool Exists => Number >= 0 && Native.Api->PlantInfo(Obj.Id, (uint)Number, null, null) != 0;

	/// <summary>Where it stands in the world, at its foot; nowhere (all zeros) once it is gone.</summary>
	public Vector3 Position
	{
		get
		{
			var at = Vector3.Zero;
			if (Number >= 0)
			{
				Native.Api->PlantInfo(Obj.Id, (uint)Number, &at, null);
			}
			return at;
		}
	}

	/// <summary>How large it is, as its Foliage grew it and its Obj scales it; 0 once it is gone.</summary>
	public float Size
	{
		get
		{
			var size = 0f;
			if (Number >= 0)
			{
				Native.Api->PlantInfo(Obj.Id, (uint)Number, null, &size);
			}
			return size;
		}
	}

	/// <summary>Takes it away, leaving nothing. Returns whether it was there.</summary>
	public bool Remove()
	{
		return Number >= 0 && Native.Api->PlantRemove(Obj.Id, (uint)Number) != 0;
	}

	/// <summary>
	/// Cuts it down: takes it away and makes its Foliage's <b>Cut</b> prefab where it stood, turned as it was and as
	/// large, a falling trunk and a stump, say. Returns the prefab's root, or <see cref="Obj.None"/> when the plant was
	/// not there or its Foliage has no Cut prefab.
	/// </summary>
	public Obj CutDown()
	{
		if (Number < 0)
		{
			return Obj.None;
		}
		var root = Native.CutPlant(Obj, Number, out var handles);
		Activation.Run(handles);
		return root;
	}

	public bool Equals(Plant other) => Obj == other.Obj && Number == other.Number;

	public override bool Equals(object? other) => other is Plant plant && Equals(plant);

	public override int GetHashCode() => HashCode.Combine(Obj, Number);

	public static bool operator ==(Plant a, Plant b) => a.Equals(b);

	public static bool operator !=(Plant a, Plant b) => !a.Equals(b);

	public override string ToString() => $"Plant {Number} of {Obj}";
}
