using System;

namespace Voxear;

/// <summary>
/// A mark of a Motion Set's action: a moment of it named in the set, as a grenade leaving the hand. When an action
/// passes one, behaviours on the character are told with OnMotionMark; compare the mark with one picked in the
/// Inspector.
/// <code>
/// [SerializeField] private MotionMark release;
///
/// protected override void OnMotionMark(MotionMark mark)
/// {
/// 	if (mark == release)
/// 	{
/// 		Throw();
/// 	}
/// }
/// </code>
/// </summary>
public readonly struct MotionMark : IEquatable<MotionMark>, IAsset<MotionMark>
{
	public static readonly MotionMark None = default;

	/// <summary>The set's GUID, or away from the engine its path.</summary>
	private readonly string? set;

	private readonly string? name;

	internal MotionMark(string set, string name)
	{
		this.set = set;
		this.name = name;
	}

	/// <summary>The set it is from.</summary>
	public MotionSet Set => MotionSet.FromKey(set ?? string.Empty);

	/// <summary>Its name in the set.</summary>
	public string Name => name ?? string.Empty;

	public bool IsValid => !string.IsNullOrEmpty(name);

	string? IAsset<MotionMark>.AssetGuid => IsValid ? $"{set}#{MotionParts.Mark}{name}" : null;

	/// <summary>
	/// A mark by its reference, the set's path or GUID then the mark, for example "Sets/Hero.motionset#mark:Release". Logs an error and returns None when it names none.
	/// </summary>
	public static MotionMark Load(string reference)
	{
		return MotionParts.Split(reference, MotionParts.Mark, out var set, out var name) ? new MotionMark(set, name) : None;
	}

	public static bool operator ==(MotionMark left, MotionMark right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(MotionMark left, MotionMark right)
	{
		return !left.Equals(right);
	}

	public bool Equals(MotionMark other)
	{
		return Name == other.Name && (set ?? string.Empty) == (other.set ?? string.Empty);
	}

	public override bool Equals(object? other)
	{
		return other is MotionMark mark && Equals(mark);
	}

	public override int GetHashCode()
	{
		return HashCode.Combine(set ?? string.Empty, Name);
	}

	public override string ToString()
	{
		return IsValid ? Name : "MotionMark(none)";
	}
}
