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
		return callbacks;
	}

	private static bool Overrides(Type type, string name)
	{
		var method = type.GetMethod(name, MethodFlags, null, Type.EmptyTypes, null);
		return method != null && method.DeclaringType != typeof(Behaviour);
	}
}
