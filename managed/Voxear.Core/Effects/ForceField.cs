using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Force Field: it moves the particles round its Obj, those of every Particle System that takes forces,
/// within its Radius. Get it with Obj.ForceField, or give an Obj one with Obj.AddForceField.
/// <code>
/// // A whirlwind that picks up the dust round it.
/// var whirl = Obj.AddForceField();
/// whirl.Kind = ForceKind.Vortex;
/// whirl.Strength = 15f;
/// whirl.Radius = 4f;
/// </code>
/// </summary>
public readonly struct ForceField : IEquatable<ForceField>, IObjComponent
{
	// What each value's number is, as the engine's particles.rs has them.
	private const int KindValue = 3;
	private const int StrengthValue = 4;
	private const int RadiusValue = 5;

	public static readonly ForceField None = default;

	internal ForceField(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it moves particles round.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Force Field, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativeComponent.Number(NativeComponent.ForceField, Obj, NativeComponent.Has, 0f) != 0f;

	/// <summary>What it does to the particles within its reach.</summary>
	public ForceKind Kind
	{
		get { return (ForceKind)(int)Number(KindValue); }
		set { Set(KindValue, (int)value); }
	}

	/// <summary>How hard it moves a particle at its middle, in metres a second faster each second; less further out.</summary>
	public float Strength
	{
		get { return Number(StrengthValue); }
		set { Set(StrengthValue, value); }
	}

	/// <summary>How far it reaches, in metres.</summary>
	public float Radius
	{
		get { return Number(RadiusValue); }
		set { Set(RadiusValue, value); }
	}

	/// <summary>Takes the Force Field off its Obj.</summary>
	public void Remove()
	{
		Set(NativeComponent.Remove, 0f);
	}

	public static bool operator ==(ForceField left, ForceField right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(ForceField left, ForceField right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(ForceField other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is ForceField field && Equals(field);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Force Field";
	}

	private float Number(int what)
	{
		return NativeComponent.Number(NativeComponent.ForceField, Obj, what, 0f);
	}

	private void Set(int what, float value)
	{
		NativeComponent.Set(NativeComponent.ForceField, Obj, what, new Vector4(value, 0f, 0f, 0f));
	}
}
