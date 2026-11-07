using System;

namespace Voxear;

/// <summary>
/// An action of a Gun Handling's own, as the Gun Editor makes it: looking a rifle over, clearing a jam, or a
/// built-in action made over. Do one with a Gun Holder's Do; pick it in the Inspector from the project's handlings.
/// <code>
/// [SerializeField] private GunHandlingAction inspect;
///
/// protected override void Update()
/// {
/// 	if (Input.Pressed("Inspect"))
/// 	{
/// 		Obj.GunHolder.Do(inspect);
/// 	}
/// }
/// </code>
/// </summary>
public readonly struct GunHandlingAction : IEquatable<GunHandlingAction>, IAsset<GunHandlingAction>
{
	public static readonly GunHandlingAction None = default;

	/// <summary>The handling's GUID, or away from the engine its path.</summary>
	private readonly string? handling;

	private readonly string? name;

	internal GunHandlingAction(string handling, string name)
	{
		this.handling = handling;
		this.name = name;
	}

	/// <summary>The handling it is in.</summary>
	public GunHandling Handling => GunHandling.FromKey(handling ?? string.Empty);

	/// <summary>Its name in the handling.</summary>
	public string Name => name ?? string.Empty;

	public bool IsValid => !string.IsNullOrEmpty(name);

	internal string Reference => $"{handling}#{MotionParts.Action}{name}";

	string? IAsset<GunHandlingAction>.AssetGuid => IsValid ? Reference : null;

	/// <summary>
	/// An action by its reference, the handling's path or GUID then the action, for example
	/// "Guns/Rifle.gunhandling#action:Inspect". Logs an error and returns None when it names none.
	/// </summary>
	public static GunHandlingAction Load(string reference)
	{
		var at = reference.IndexOf('#' + MotionParts.Action, StringComparison.Ordinal);
		var found = at > 0 ? MotionParts.SetKey(reference[..at]) : null;
		if (found == null || at + 1 + MotionParts.Action.Length >= reference.Length)
		{
			Log.Error($"\"{reference}\" names no action of a Gun Handling.");
			return None;
		}
		return new GunHandlingAction(found, reference[(at + 1 + MotionParts.Action.Length)..]);
	}

	internal static GunHandlingAction FromReference(string? reference)
	{
		if (string.IsNullOrEmpty(reference))
		{
			return None;
		}
		var at = reference.IndexOf('#' + MotionParts.Action, StringComparison.Ordinal);
		return at > 0 ? new GunHandlingAction(reference[..at], reference[(at + 1 + MotionParts.Action.Length)..]) : None;
	}

	public static bool operator ==(GunHandlingAction left, GunHandlingAction right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(GunHandlingAction left, GunHandlingAction right)
	{
		return !left.Equals(right);
	}

	public bool Equals(GunHandlingAction other)
	{
		return Name == other.Name && (handling ?? string.Empty) == (other.handling ?? string.Empty);
	}

	public override bool Equals(object? other)
	{
		return other is GunHandlingAction action && Equals(action);
	}

	public override int GetHashCode()
	{
		return HashCode.Combine(handling ?? string.Empty, Name);
	}

	public override string ToString()
	{
		return IsValid ? Name : "GunHandlingAction(none)";
	}
}
