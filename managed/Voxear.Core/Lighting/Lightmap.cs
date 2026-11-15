using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Lightmap: its Mesh Renderer or Model Renderer keeps still for lighting, so Bake Lighting gives its
/// meshes a lightmap, the light falling on each point of them, traced once. Get it with Obj.Lightmap, or give an
/// Obj one with Obj.AddLightmap.
/// <code>
/// var wall = Obj.Find("Hall Wall");
/// wall.AddLightmap().Scale = 2f; // twice the detail each way
/// </code>
/// </summary>
public readonly struct Lightmap : IEquatable<Lightmap>, IObjComponent
{
	private const int ScaleValue = 3;
	private const int CastsValue = 4;
	private const int ReceivesValue = 5;
	private const int BakedValue = 6;

	public static readonly Lightmap None = default;

	internal Lightmap(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj that keeps still.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Lightmap, or is gone.</summary>
	public bool IsValid => Obj.IsValid && Number(NativeComponent.Has) != 0f;

	/// <summary>Multiplies the level's texels a metre for this Obj: 2 for twice the detail each way.</summary>
	public float Scale
	{
		get { return Number(ScaleValue); }
		set { Write(ScaleValue, value); }
	}

	/// <summary>Whether it shades and bounces light in the bake.</summary>
	public bool Casts
	{
		get { return Number(CastsValue) != 0f; }
		set { Write(CastsValue, value ? 1f : 0f); }
	}

	/// <summary>Whether it takes a lightmap; without, the light probes light it, as they do a moving Obj.</summary>
	public bool Receives
	{
		get { return Number(ReceivesValue) != 0f; }
		set { Write(ReceivesValue, value ? 1f : 0f); }
	}

	/// <summary>Whether the level's bake has given it a place in a lightmap.</summary>
	public bool IsBaked => Number(BakedValue) != 0f;

	/// <summary>Takes the Lightmap off its Obj, and its baked light with it.</summary>
	public void Remove()
	{
		NativeComponent.Set(NativeComponent.Lightmap, Obj, NativeComponent.Remove, Vector4.Zero);
	}

	public static bool operator ==(Lightmap left, Lightmap right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(Lightmap left, Lightmap right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(Lightmap other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is Lightmap lightmap && Equals(lightmap);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Lightmap";
	}

	private float Number(int what)
	{
		return NativeComponent.Number(NativeComponent.Lightmap, Obj, what, 0f);
	}

	private void Write(int what, float value)
	{
		var written = NativeComponent.Set(NativeComponent.Lightmap, Obj, what, new Vector4(value, 0f, 0f, 0f));
		if (!written && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Lightmap; give it one with AddLightmap.");
		}
	}
}
