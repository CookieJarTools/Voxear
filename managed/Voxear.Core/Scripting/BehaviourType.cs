using System;
using System.Reflection;

namespace Voxear.Scripting;

/// <summary>
/// Everything the engine needs from a behaviour class, worked out once per type. Only the phases a
/// type overrides are reported, so the engine never calls an empty method.
/// </summary>
internal sealed class BehaviourType
{
	private const BindingFlags MethodFlags = BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic;

	/// <summary>A mark beside the callbacks, as the engine's Callbacks::PREDICTED has it, for a [Predicted] behaviour.</summary>
	private const uint PredictedMark = 1u << 16;

	public BehaviourType(Type type)
	{
		Type = type;
		Callbacks = FindCallbacks(type);
		Order = type.GetCustomAttribute<ExecutionOrderAttribute>()?.Order ?? 0;
		IsInstance = typeof(IInstance).IsAssignableFrom(type);
	}

	public Type Type { get; }

	public uint Callbacks { get; }

	public int Order { get; }

	public bool IsInstance { get; }

	public Behaviour Create()
	{
		return (Behaviour)Activator.CreateInstance(Type)!;
	}

	private static uint FindCallbacks(Type type)
	{
		uint callbacks = 0;
		foreach (var phase in Enum.GetValues<Phase>())
		{
			if (Overrides(type, phase.ToString()))
			{
				callbacks |= 1u << (int)phase;
			}
		}
		if (IsPredicted(type))
		{
			callbacks |= PredictedMark;
		}
		return callbacks;
	}

	/// <summary>Whether a class has the Networking module's [Predicted], which Voxear.Core knows only by name.</summary>
	private static bool IsPredicted(Type type)
	{
		foreach (var attribute in type.GetCustomAttributesData())
		{
			if (attribute.AttributeType.FullName == "Voxear.PredictedAttribute")
			{
				return true;
			}
		}
		return false;
	}

	private static bool Overrides(Type type, string name)
	{
		var method = type.GetMethod(name, MethodFlags, null, Type.EmptyTypes, null);
		return method != null && method.DeclaringType != typeof(Behaviour);
	}
}
