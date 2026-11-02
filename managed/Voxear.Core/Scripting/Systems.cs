using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using Voxear.Interop;

namespace Voxear.Scripting;

/// <summary>
/// The game's systems, one of each SystemBase class in its scripts, by stage. They are made for the world the
/// engine lends when they first run, and made again when that world changes, as each time play mode starts, since
/// what they keep belongs to the old one. A reload destroys them first, so the new build's are made next frame.
/// </summary>
internal static unsafe class Systems
{
	private static readonly List<SystemBase>[] byStage = Enumerable.Range(0, (int)Stage.Last + 1)
		.Select(_ => new List<SystemBase>())
		.ToArray();

	private static ulong world;
	private static bool made;

	public static void Run(Stage stage)
	{
		var current = *Native.Api->WorldId;
		if (!made || current != world)
		{
			Clear();
			Make();
			world = current;
			made = true;
		}

		foreach (var system in byStage[(int)stage])
		{
			if (system.Enabled)
			{
				Update(system);
			}
		}
	}

	/// <summary>Gives every system OnDestroy and lets them go, as before a reload, while their world is still lent.</summary>
	public static void Destroy()
	{
		foreach (var system in byStage.SelectMany(systems => systems))
		{
			Guarded(system.RunDestroy);
		}
		Forget();
	}

	/// <summary>Lets the systems go without OnDestroy, so the next frame makes them from the scripts loaded then.</summary>
	public static void Forget()
	{
		Clear();
		made = false;
	}

	private static void Clear()
	{
		foreach (var systems in byStage)
		{
			systems.Clear();
		}
	}

	private static void Make()
	{
		var types = GameScripts.Assemblies
			.SelectMany(Loadable)
			.Where(IsSystem)
			.OrderBy(type => type.GetCustomAttribute<ExecutionOrderAttribute>()?.Order ?? 0)
			.ThenBy(type => type.FullName, StringComparer.Ordinal);
		foreach (var type in types)
		{
			var system = (SystemBase)Activator.CreateInstance(type)!;
			system.Marker = Native.ProfilerMarker($"{type.FullName}.Update()");
			var stage = type.GetCustomAttribute<UpdateInAttribute>()?.Stage ?? Stage.Update;
			byStage[(int)stage].Add(system);
		}

		foreach (var system in byStage.SelectMany(systems => systems))
		{
			Guarded(system.RunCreate);
		}
	}

	/// <summary>
	/// One system's Update, as a Profiler sample of its own. What it adds, removes and destroys waits until it
	/// returns, and its changes from then on count as after it ran.
	/// </summary>
	private static void Update(SystemBase system)
	{
		var sampled = Native.ProfilerBegin(system.Marker);
		Native.Api->CommandsDefer(1);
		try
		{
			system.RunUpdate();
		}
		catch (Exception exception)
		{
			Log.Exception(exception);
		}
		finally
		{
			Native.Api->CommandsDefer(0);
			if (sampled)
			{
				Native.ProfilerEnd();
			}
		}
		system.LastRun = Native.Api->ChangeTick(1);
	}

	private static bool IsSystem(Type type)
	{
		return type.IsSubclassOf(typeof(SystemBase)) && !type.IsAbstract && type.GetConstructor(Type.EmptyTypes) != null;
	}

	private static IEnumerable<Type> Loadable(Assembly assembly)
	{
		try
		{
			return assembly.GetTypes();
		}
		catch (ReflectionTypeLoadException exception)
		{
			return exception.Types.Where(type => type != null)!;
		}
	}

	private static void Guarded(Action action)
	{
		try
		{
			action();
		}
		catch (Exception exception)
		{
			Log.Exception(exception);
		}
	}
}
