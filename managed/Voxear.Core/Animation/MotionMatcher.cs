using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Motion Matcher: it animates the Obj's model by blending whichever of the model's looping clips move as
/// the character is wanted to, by speed and direction, and stands with a still clip below a walk. The game moves the
/// Obj, with a Character Controller or otherwise, and the model stands on it, its body turning the way it goes. Left
/// alone it follows how the Obj moves; tell it where the player is heading with WantedVelocity, and where to face with
/// WantedFacing to step aside or back while aiming. Get it with Obj.MotionMatcher, or give an Obj one with
/// Obj.AddMotionMatcher.
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

	/// <summary>
	/// The way the game wants the body to face, as when aiming while stepping aside or back. Zero faces the way it
	/// goes.
	/// </summary>
	public Vector3 WantedFacing
	{
		get { return Vector(NativeMotionMatcher.WantedFacing); }
		set { Set(NativeMotionMatcher.WantedFacing, new Vector4(value, 0f)); }
	}

	/// <summary>
	/// How far, in degrees, the body bends at the spine to look up or down, down positive, as the view's pitch: its
	/// arms aim where it looks.
	/// </summary>
	public float AimPitch
	{
		get { return Number(NativeMotionMatcher.AimPitch); }
		set { Set(NativeMotionMatcher.AimPitch, value); }
	}

	/// <summary>The Motion Set it plays, its states and actions. None plays the model's own clips.</summary>
	public MotionSet MotionSet
	{
		get { return MotionSet.Load(NativeMotionMatcher.Text(Obj, NativeMotionMatcher.MotionSetText) ?? string.Empty); }
		set { Tell(NativeMotionMatcher.SetMotionSet, value.Key); }
	}

	/// <summary>
	/// What the state it plays is for, as walking about or crouching. Setting it plays the set's first state for that,
	/// changing over the state's Blend Time and playing its Enter Action, from the next frame.
	/// </summary>
	public MotionState State
	{
		get { return (MotionState)Math.Max((int)Number(NativeMotionMatcher.StateRole), 0); }
		set { Set(NativeMotionMatcher.StateRole, (int)value); }
	}

	/// <summary>The set's state it plays, or from the next frame plays as asked.</summary>
	public MotionSetState CurrentState
	{
		get
		{
			var name = NativeMotionMatcher.Text(Obj, NativeMotionMatcher.StateName);
			return name == null ? MotionSetState.None : new MotionSetState(MotionSet.Key, name);
		}
	}

	/// <summary>
	/// The action it plays, or plays from the next frame as asked: None with none, or once it is stopped and fading
	/// out.
	/// </summary>
	public MotionAction ActionPlaying
	{
		get
		{
			var name = NativeMotionMatcher.Text(Obj, NativeMotionMatcher.ActionName);
			return name == null ? MotionAction.None : new MotionAction(MotionSet.Key, name);
		}
	}

	/// <summary>How far into the action it plays it is, in seconds; 0 with none.</summary>
	public float ActionTime => Number(NativeMotionMatcher.ActionTime);

	/// <summary>
	/// Whether the clips move the Obj as they travel, through its Character Controller when it has one, rather than
	/// the game moving it. A clip set to move at a speed and direction moves it at those.
	/// </summary>
	public bool RootMotion
	{
		get { return Number(NativeMotionMatcher.RootMotion) != 0f; }
		set { Set(NativeMotionMatcher.RootMotion, value ? 1f : 0f); }
	}

	/// <summary>Below this many metres a second wanted, it stands.</summary>
	public float IdleSpeed
	{
		get { return Number(NativeMotionMatcher.IdleSpeed); }
		set { Set(NativeMotionMatcher.IdleSpeed, value); }
	}

	/// <summary>Seconds between searches for the clips to blend.</summary>
	public float SearchEvery
	{
		get { return Number(NativeMotionMatcher.SearchEvery); }
		set { Set(NativeMotionMatcher.SearchEvery, value); }
	}

	/// <summary>How long, in seconds, a change of clips takes to fade in.</summary>
	public float BlendTime
	{
		get { return Number(NativeMotionMatcher.Blend); }
		set { Set(NativeMotionMatcher.Blend, value); }
	}

	/// <summary>How fast the body turns, in degrees a second; 0 turns at once.</summary>
	public float TurnRate
	{
		get { return Number(NativeMotionMatcher.TurnRate); }
		set { Set(NativeMotionMatcher.TurnRate, value); }
	}

	/// <summary>Whether the body faces the way it goes when no WantedFacing is set, rather than the Obj's forward.</summary>
	public bool TurnToMovement
	{
		get { return Number(NativeMotionMatcher.TurnToMovement) != 0f; }
		set { Set(NativeMotionMatcher.TurnToMovement, value ? 1f : 0f); }
	}

	/// <summary>
	/// How far, in degrees, the hips and legs may turn from the way the body faces, towards the way it goes, its spine
	/// twisting back so the chest still faces ahead; 0 keeps them with the body.
	/// </summary>
	public float LegTurn
	{
		get { return Number(NativeMotionMatcher.LegTurn); }
		set { Set(NativeMotionMatcher.LegTurn, value); }
	}

	/// <summary>Whether clips play a little faster or slower so their feet keep pace with the ground.</summary>
	public bool MatchSpeed
	{
		get { return Number(NativeMotionMatcher.MatchSpeed) != 0f; }
		set { Set(NativeMotionMatcher.MatchSpeed, value ? 1f : 0f); }
	}

	/// <summary>Whether each foot stays where it comes down while the clips have it down, the leg bending to reach it.</summary>
	public bool FootLocking
	{
		get { return Number(NativeMotionMatcher.FootLocking) != 0f; }
		set { Set(NativeMotionMatcher.FootLocking, value ? 1f : 0f); }
	}

	/// <summary>Whether it draws the velocity wanted, the clips' velocity and the body's facing, and names the clips.</summary>
	public bool Debug
	{
		get { return Number(NativeMotionMatcher.Debug) != 0f; }
		set { Set(NativeMotionMatcher.Debug, value ? 1f : 0f); }
	}

	/// <summary>
	/// Plays an action of its Motion Set from the next frame, in place of any playing: it fades in over the state,
	/// plays once, and fades out at its end, unless it loops or holds its last frame.
	/// </summary>
	public void Play(MotionAction action)
	{
		if (InSet(action.Set, action.IsValid, "action"))
		{
			Tell(NativeMotionMatcher.Play, action.Name);
		}
	}

	/// <summary>Fades out the action playing, as one held at its end.</summary>
	public void StopAction()
	{
		Tell(NativeMotionMatcher.Stop, string.Empty);
	}

	/// <summary>Plays a state of its Motion Set from the next frame, as a custom state, changing over its Blend Time.</summary>
	public void SetState(MotionSetState state)
	{
		if (InSet(state.Set, state.IsValid, "state"))
		{
			Tell(NativeMotionMatcher.SetState, state.Name);
		}
	}

	/// <summary>Forgets WantedVelocity and WantedFacing, following how the Obj moves.</summary>
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

	/// <summary>Whether a part named is of the set it plays, logging why not.</summary>
	private bool InSet(MotionSet set, bool named, string part)
	{
		if (!named)
		{
			Log.Error($"No {part} is set to play on {Obj}'s Motion Matcher.");
			return false;
		}

		var playing = MotionSet;
		if (set != playing && Native.IsConnected)
		{
			Log.Error($"That {part} is of another Motion Set than {Obj}'s Motion Matcher plays.");
			return false;
		}
		return true;
	}

	private void Tell(int what, string text)
	{
		if (!NativeMotionMatcher.Tell(Obj, what, text) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Motion Matcher; give it one with AddMotionMatcher.");
		}
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
