using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Motion Matcher: it animates the Obj's model by playing whichever frame of the model's clips best carries
/// on from its pose into the path the character is about to take. The game moves the Obj, with a Character
/// Controller or otherwise, and the body follows. Left alone it expects the Obj to carry on as it moves; tell it
/// where the player is heading with WantedVelocity, and it turns and slows before the Obj does. Get it with
/// Obj.MotionMatcher, or give an Obj one with Obj.AddMotionMatcher.
/// <code>
/// // Walking at 1.4 metres a second where the keys point.
/// var ahead = (Keyboard.IsDown(Key.W) ? 1f : 0f) - (Keyboard.IsDown(Key.S) ? 1f : 0f);
/// var aside = (Keyboard.IsDown(Key.D) ? 1f : 0f) - (Keyboard.IsDown(Key.A) ? 1f : 0f);
/// var matcher = Obj.MotionMatcher;
/// matcher.WantedVelocity = (Obj.Forward * ahead + Obj.Right * aside) * 1.4f;
/// </code>
/// </summary>
public readonly struct MotionMatcher : IEquatable<MotionMatcher>, IObjComponent
{
	public static readonly MotionMatcher None = default;

	internal MotionMatcher(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj whose model it animates.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Motion Matcher, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativeMotionMatcher.Number(Obj, NativeMotionMatcher.Has, 0f) != 0f;

	/// <summary>The velocity the game wants, in metres a second. Zero until set; FollowObj forgets it.</summary>
	public Vector3 WantedVelocity
	{
		get { return Vector(NativeMotionMatcher.WantedVelocity); }
		set { Set(NativeMotionMatcher.WantedVelocity, new Vector4(value, 0f)); }
	}

	/// <summary>The way the game wants the character to face. Zero until set, the Obj's forward meanwhile.</summary>
	public Vector3 WantedFacing
	{
		get { return Vector(NativeMotionMatcher.WantedFacing); }
		set { Set(NativeMotionMatcher.WantedFacing, new Vector4(value, 0f)); }
	}

	/// <summary>How quickly the character is expected to reach what is wanted, in seconds to halve the gap.</summary>
	public float Response
	{
		get { return Number(NativeMotionMatcher.Response); }
		set { Set(NativeMotionMatcher.Response, value); }
	}

	/// <summary>Seconds between searches for a better frame.</summary>
	public float SearchEvery
	{
		get { return Number(NativeMotionMatcher.SearchEvery); }
		set { Set(NativeMotionMatcher.SearchEvery, value); }
	}

	/// <summary>How long, in seconds, a jump to a new frame takes to settle.</summary>
	public float BlendTime
	{
		get { return Number(NativeMotionMatcher.Blend); }
		set { Set(NativeMotionMatcher.Blend, value); }
	}

	/// <summary>How far, in metres, the body may stray from the Obj.</summary>
	public float MaxDrift
	{
		get { return Number(NativeMotionMatcher.MaxDrift); }
		set { Set(NativeMotionMatcher.MaxDrift, value); }
	}

	/// <summary>How far, in degrees, the body may turn from the Obj's facing.</summary>
	public float MaxTurn
	{
		get { return Number(NativeMotionMatcher.MaxTurn); }
		set { Set(NativeMotionMatcher.MaxTurn, value); }
	}

	/// <summary>Whether it draws the path wanted, the path of the frame playing, and the body's place.</summary>
	public bool Debug
	{
		get { return Number(NativeMotionMatcher.Debug) != 0f; }
		set { Set(NativeMotionMatcher.Debug, value ? 1f : 0f); }
	}

	/// <summary>How far, in metres, the body stands from the Obj now.</summary>
	public float Drift => Number(NativeMotionMatcher.Drift);

	/// <summary>Forgets WantedVelocity and WantedFacing, expecting the Obj to carry on as it moves.</summary>
	public void FollowObj()
	{
		Set(NativeMotionMatcher.Follow, Vector4.Zero);
	}

	/// <summary>Takes the Motion Matcher off its Obj, leaving its model as it was posed.</summary>
	public void Remove()
	{
		NativeMotionMatcher.Set(Obj, NativeMotionMatcher.Remove, Vector4.Zero);
	}

	public static bool operator ==(MotionMatcher left, MotionMatcher right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(MotionMatcher left, MotionMatcher right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(MotionMatcher other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is MotionMatcher matcher && Equals(matcher);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Motion Matcher";
	}

	private float Number(int what)
	{
		return NativeMotionMatcher.Number(Obj, what, 0f);
	}

	private Vector3 Vector(int what)
	{
		return NativeMotionMatcher.Get(Obj, what, out var value) ? new Vector3(value.X, value.Y, value.Z) : Vector3.Zero;
	}

	private void Set(int what, float value)
	{
		Set(what, new Vector4(value, 0f, 0f, 0f));
	}

	private void Set(int what, Vector4 value)
	{
		if (!NativeMotionMatcher.Set(Obj, what, value) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Motion Matcher; give it one with AddMotionMatcher.");
		}
	}
}
