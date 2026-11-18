using System;

namespace Voxear;

/// <summary>
/// One of the project's agent types, set in Project Settings ▸ Navigation: the Default one, or one of the project's
/// own four. A Nav Agent's type says what each kind of ground costs it and which of a level's baked nav meshes it
/// walks.
/// <code>
/// // The ogre is the project's first own type, which Project Settings names Ogre.
/// ogre.NavAgent.AgentType = NavAgentType.Own(1);
/// </code>
/// </summary>
public readonly struct NavAgentType : IEquatable<NavAgentType>
{
	/// <summary>How many of its own agent types a project has.</summary>
	public const int OwnCount = 4;

	/// <summary>The Default agent type.</summary>
	public static readonly NavAgentType Default = default;

	private NavAgentType(int slot)
	{
		Slot = slot;
	}

	/// <summary>Its place in Project Settings ▸ Navigation: 0 for the Default one, 1 to 4 for the project's own.</summary>
	public int Slot { get; }

	/// <summary>Whether it is the Default agent type.</summary>
	public bool IsDefault => Slot == 0;

	/// <summary>The project's own agent type `number`, from 1 to 4; the Default one for any other.</summary>
	public static NavAgentType Own(int number)
	{
		return number is >= 1 and <= OwnCount ? new NavAgentType(number) : Default;
	}

	internal static NavAgentType OfSlot(int slot)
	{
		return Own(slot);
	}

	public static bool operator ==(NavAgentType left, NavAgentType right)
	{
		return left.Slot == right.Slot;
	}

	public static bool operator !=(NavAgentType left, NavAgentType right)
	{
		return left.Slot != right.Slot;
	}

	public bool Equals(NavAgentType other)
	{
		return Slot == other.Slot;
	}

	public override bool Equals(object? other)
	{
		return other is NavAgentType type && Equals(type);
	}

	public override int GetHashCode()
	{
		return Slot;
	}

	public override string ToString()
	{
		return IsDefault ? "Default agent type" : $"Agent type {Slot}";
	}
}
