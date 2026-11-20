using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Dry Area, which keeps the sea, Pools and Streams out of its shape: a ship's hull, a diving bell, a cave
/// under the sea. Its Bilge is water standing in it, level however the hull rocks: set it each frame and the water
/// drawn in it, what scripts find there with Water.TrySample, and the Buoyancy of what floats in it all follow it
/// on the same frame, so a hold can flood smoothly over seconds. Get it with Obj.DryArea, null when the Obj has
/// none, or give an Obj one with Obj.AddDryArea.
/// <code>
/// // A hold that floods through a hole in the hull, a little more each second, and is pumped dry.
/// protected override void Update()
/// {
///     if (Obj.DryArea is { } hold)
///     {
///         hold.BilgeFill = MathF.Min(hold.BilgeFill + 0.02f * Time.Delta, 0.8f);
///     }
/// }
///
/// public void Pump() => Obj.DryArea?.Bilge = 0f;
/// </code>
/// </summary>
public sealed class DryArea : IEquatable<DryArea>, IObjComponent
{
	private const int ShapeValue = 3;
	private const int SizeValue = 4;
	private const int BilgeValue = 5;
	private const int BilgeFillValue = 6;
	private const int BilgeEnabledValue = 7;
	private const int InsideHeightValue = 8;

	internal DryArea(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj whose Dry Area it is.</summary>
	public Obj Obj { get; }

	/// <summary>False once its Obj has no Dry Area, or is gone.</summary>
	public bool IsValid => Obj.IsValid && Number(NativeComponent.Has) != 0f;

	/// <summary>The shape it keeps water out of.</summary>
	public DryShape Shape
	{
		get { return (DryShape)(int)Number(ShapeValue); }
		set { Write(ShapeValue, new Vector4((float)value, 0f, 0f, 0f)); }
	}

	/// <summary>
	/// How big its shape is, in metres: a box's three sides; a ball's width in X; a capsule's width in X and height in
	/// Y. A Mesh goes by its mesh instead.
	/// </summary>
	public Vector3 Size
	{
		get
		{
			NativeComponent.Get(NativeComponent.DryArea, Obj, SizeValue, out var size);
			return new Vector3(size.X, size.Y, size.Z);
		}
		set { Write(SizeValue, new Vector4(value, 0f)); }
	}

	/// <summary>
	/// How deep the water standing in it is, in metres from its bottom, as high over its middle however it rocks: 0
	/// for none. It can be no deeper than InsideHeight.
	/// </summary>
	public float Bilge
	{
		get { return Number(BilgeValue); }
		set { Write(BilgeValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>Its Bilge as a share of how tall its inside stands, from 0 for dry to 1 for full to the top.</summary>
	public float BilgeFill
	{
		get { return Number(BilgeFillValue); }
		set { Write(BilgeFillValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>Whether its Bilge is there: off, it is dry inside, its depth kept for when it is on again.</summary>
	public bool BilgeEnabled
	{
		get { return Number(BilgeEnabledValue) != 0f; }
		set { Write(BilgeEnabledValue, new Vector4(value ? 1f : 0f, 0f, 0f, 0f)); }
	}

	/// <summary>
	/// How tall its inside stands, in metres: the deepest its Bilge can be. A Mesh's is 0 until its mesh has loaded,
	/// and in a game with no window to draw in.
	/// </summary>
	public float InsideHeight => Number(InsideHeightValue);

	/// <summary>
	/// The mesh a Mesh Dry Area keeps water out of, in place of the mesh its Obj draws; None for its Obj's own. Its
	/// InsideHeight follows it once it has loaded.
	/// </summary>
	public MeshAsset Mesh
	{
		get { return MeshAsset.FromReference(NativeWhole.Asset(NativeWhole.DryAreaMesh, Obj)); }
		set
		{
			if (!NativeWhole.SetAsset(NativeWhole.DryAreaMesh, Obj, value.IsSet ? value.Reference : string.Empty) && Native.IsConnected && !IsValid)
			{
				Log.Error($"{Obj} has no Dry Area; give it one with AddDryArea.");
			}
		}
	}

	/// <summary>Takes the Dry Area off its Obj, which lets the water back in.</summary>
	public void Remove()
	{
		NativeComponent.Set(NativeComponent.DryArea, Obj, NativeComponent.Remove, Vector4.Zero);
	}

	public static bool operator ==(DryArea? left, DryArea? right)
	{
		return left is null ? right is null : left.Equals(right);
	}

	public static bool operator !=(DryArea? left, DryArea? right)
	{
		return !(left == right);
	}

	public bool Equals(DryArea? other)
	{
		return other is not null && Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is DryArea area && Equals(area);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	/// <summary>The Obj's Dry Area, or null when it has none.</summary>
	internal static DryArea? Of(Obj obj)
	{
		var area = new DryArea(obj);
		return area.IsValid ? area : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s Dry Area";
	}

	private float Number(int what)
	{
		return NativeComponent.Number(NativeComponent.DryArea, Obj, what, 0f);
	}

	private void Write(int what, Vector4 value)
	{
		var written = NativeComponent.Set(NativeComponent.DryArea, Obj, what, value);
		if (!written && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Dry Area; give it one with AddDryArea.");
		}
	}
}
