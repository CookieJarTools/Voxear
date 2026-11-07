using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Runtime.Loader;
using Voxear.Interop;
using Voxear.Saving;

namespace Voxear.Scripting;

/// <summary>
/// The game assemblies, loaded into one collectible context, and loaded again into a new one when they change: see
/// Reload. The game's scripts come with the assemblies of the project's packages (see PackageAssemblies), so what a
/// package defines is found as the game's own is. They are read into memory rather than mapped from their files, so
/// the next build can write over the files while this one runs.
/// </summary>
internal static class GameScripts
{
	/// <summary>How many frames after a reload to look whether the old scripts have left memory.</summary>
	private const int SettleFrames = 120;

	/// <summary>How many values that could not be carried over a reload are named, before the rest are counted.</summary>
	private const int LostNamed = 12;

	private static readonly List<Assembly> assemblies = new List<Assembly>();
	private static readonly List<WeakReference> retired = new List<WeakReference>();
	private static GameLoadContext? context;
	private static int settling;

	public static IReadOnlyList<Assembly> Assemblies => assemblies;

	public static void Load(string path)
	{
		context ??= new GameLoadContext(path);
		var loaded = LoadAll(context, path);
		assemblies.AddRange(loaded);
		Systems.Forget();
		SceneComponentTypes.Clear();
		BehaviourTypes.Clear();
		BehaviourFields.Clear();
		SaveTypes.Clear();
		var packages = loaded.Count - 1;
		Log.Info(packages == 0 ? $"Loaded game scripts from {path}" : $"Loaded game scripts from {path}, with {packages} package assemblies");
	}

	/// <summary>
	/// Loads a new build of the game scripts in place of the one running and keeps the game going, as Unity does when
	/// scripts change in play mode. Every enabled behaviour gets OnDisable; each behaviour is made again from its class
	/// in the new build, under the same handle, with its fields carried over (see StateTransfer); and the enabled
	/// ones get OnEnable. Awake and Start do not run again. Static fields start afresh, listeners the old scripts
	/// added to Level's events are dropped, and awaits they were in the middle of never resume, so code that
	/// subscribes in OnEnable and unsubscribes in OnDisable carries on as before. A behaviour whose class is gone is
	/// dropped, with a warning. handles are the behaviours the engine schedules, enabled says which of them are
	/// enabled, and infos is filled with what the engine needs to schedule each one now. Returns false, with nothing
	/// changed, when the new build cannot be loaded.
	/// </summary>
	public static bool Reload(string path, ReadOnlySpan<uint> handles, ReadOnlySpan<uint> enabled, Span<BehaviourInfo> infos)
	{
		var previous = context;
		var next = new GameLoadContext(path);
		List<Assembly> loaded;
		try
		{
			loaded = LoadAll(next, path);
		}
		catch (Exception exception) when (exception is IOException or BadImageFormatException)
		{
			next.Unload();
			Log.Error($"The new build of the scripts could not be loaded, so the old one keeps running. {exception.Message}");
			return false;
		}

		var live = Enabled(handles, enabled);
		Dispatch(live, Phase.OnDisable);
		Systems.Destroy();
		UiElements.Forget();
		Swap(next, loaded);
		var transfer = new StateTransfer(previous ?? next, assemblies);
		var gone = BehaviourTable.Replace(transfer);
		Forget(previous);
		for (var i = 0; i < handles.Length; i++)
		{
			infos[i] = BehaviourTable.Describe(handles[i]);
		}
		Dispatch(live, Phase.OnEnable);
		Report(transfer, gone);
		return true;
	}

	/// <summary>
	/// Once a reload has settled, collects the old scripts, and says so when something still holds them in memory:
	/// a thread or timer of theirs still running, or a static field outside the scripts pointing into them.
	/// </summary>
	public static void Settle()
	{
		if (settling == 0 || --settling > 0)
		{
			return;
		}

		GC.Collect();
		GC.WaitForPendingFinalizers();
		GC.Collect();
		retired.RemoveAll(reference => !reference.IsAlive);
		if (retired.Count > 0)
		{
			Log.Warning("The scripts from before the last reload are still in memory, so something still holds them: a thread or timer of theirs, or a static field outside the scripts. Restarting the editor frees them.");
		}
	}

	/// <summary>Whether a callback posted to the main thread would run code of scripts that have been reloaded since.</summary>
	public static bool IsRetired(object callback, object? state)
	{
		foreach (var reference in retired)
		{
			if (reference.Target is AssemblyLoadContext old && (GameCode.Holds(callback, old) || GameCode.Holds(state, old)))
			{
				return true;
			}
		}
		return false;
	}

	/// <summary>The game's assembly and its packages', read into a context: the packages first, each after the ones it uses.</summary>
	private static List<Assembly> LoadAll(AssemblyLoadContext target, string path)
	{
		var loaded = new List<Assembly>();
		foreach (var package in PackageAssemblies.Beside(path))
		{
			loaded.Add(LoadInto(target, package));
		}
		loaded.Add(LoadInto(target, path));
		foreach (var assembly in loaded)
		{
			ModuleAssemblies.LoadFor(assembly);
		}
		return loaded;
	}

	/// <summary>Reads an assembly into a context, with its symbols when they are there so stack traces keep their lines.</summary>
	private static Assembly LoadInto(AssemblyLoadContext target, string path)
	{
		using var image = new MemoryStream(File.ReadAllBytes(path));
		var symbols = Path.ChangeExtension(path, ".pdb");
		if (!File.Exists(symbols))
		{
			return target.LoadFromStream(image);
		}

		using var pdb = new MemoryStream(File.ReadAllBytes(symbols));
		return target.LoadFromStream(image, pdb);
	}

	private static uint[] Enabled(ReadOnlySpan<uint> handles, ReadOnlySpan<uint> enabled)
	{
		var live = new List<uint>();
		for (var i = 0; i < handles.Length; i++)
		{
			if (enabled[i] != 0)
			{
				live.Add(handles[i]);
			}
		}
		return live.ToArray();
	}

	/// <summary>Runs a phase on the behaviours under these handles that have it, by execution order.</summary>
	private static void Dispatch(uint[] handles, Phase phase)
	{
		foreach (var behaviour in BehaviourTable.Ordered(handles))
		{
			var type = BehaviourTypes.Find(behaviour.GetType().FullName!);
			if (type != null && (type.Callbacks & 1u << (int)phase) != 0)
			{
				BehaviourTable.Run(behaviour, phase);
			}
		}
	}

	private static void Swap(GameLoadContext next, List<Assembly> loaded)
	{
		context = next;
		assemblies.Clear();
		assemblies.AddRange(loaded);
		BehaviourTypes.Clear();
		BehaviourFields.Clear();
		SceneComponentTypes.Clear();
		InstanceRegistry.Clear();
		InspectorDrawers.Forget();
		EditorWindows.Forget();
		SceneGizmos.Forget();
		SaveTypes.Clear();
	}

	/// <summary>Lets go of the old scripts, so they can leave memory once nothing runs them.</summary>
	private static void Forget(GameLoadContext? previous)
	{
		if (previous == null)
		{
			return;
		}

		Level.Forget(previous);
		ModuleHooks.Forget?.Invoke(previous);
		SaveTypes.Forget(previous);
		MainThreadContext.Instance.Forget(previous);
		previous.Unload();
		retired.Add(new WeakReference(previous));
		settling = SettleFrames;
	}

	private static void Report(StateTransfer transfer, List<string> gone)
	{
		var carried = transfer.Replaced.Count;
		Log.Info(carried == 0 ? "Reloaded the game scripts" : $"Reloaded the game scripts, carrying over {carried} behaviours");
		if (gone.Count > 0)
		{
			Log.Warning($"These classes are not in the new build, so their behaviours were removed: {string.Join(", ", gone.Distinct())}");
		}
		if (transfer.Lost.Count == 0)
		{
			return;
		}

		var named = string.Join(", ", transfer.Lost.Take(LostNamed));
		var more = transfer.Lost.Count > LostNamed ? $", and {transfer.Lost.Count - LostNamed} more" : string.Empty;
		Log.Warning($"These could not be carried over the reload, so they start again as their class sets them: {named}{more}");
	}
}
