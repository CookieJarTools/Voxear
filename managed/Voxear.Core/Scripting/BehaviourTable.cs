using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using Voxear.Interop;

namespace Voxear.Scripting
{
	/// <summary>
	/// Live behaviours, indexed by the handle the engine schedules them with. Handles are reused
	/// after release, which keeps the table dense.
	/// </summary>
	internal static class BehaviourTable
	{
		private static readonly List<Behaviour?> behaviours = new List<Behaviour?>();
		private static readonly Stack<int> free = new Stack<int>();

		/// <summary>
		/// Creates a behaviour, keeping its field values from the scene to set just before its Awake. Awake is then
		/// always dispatched, even when the type does not override it, so the fields are never skipped.
		/// </summary>
		public static unsafe int Create(string typeName, Obj obj, string? fields, BehaviourInfo* info)
		{
			var type = BehaviourTypes.Find(typeName);
			if (type == null)
			{
				Log.Error($"No Behaviour named {typeName} was found in the game scripts.");
				return -1;
			}

			var behaviour = type.Create();
			var handle = Allocate(behaviour);
			behaviour.Attach(obj, (uint)handle, type.Order);
			behaviour.PendingFields = fields;
			info->Callbacks = fields == null ? type.Callbacks : type.Callbacks | 1u << (int)Phase.Awake;
			info->Order = type.Order;
			info->Flags = type.IsInstance ? BehaviourInfo.InstanceFlag : 0;

			// Registered before any Awake runs, so every Awake in the batch can already reach it.
			if (type.IsInstance)
			{
				InstanceRegistry.Register(behaviour);
			}
			return handle;
		}

		public static Behaviour? Get(uint handle)
		{
			return handle < behaviours.Count ? behaviours[(int)handle] : null;
		}

		/// <summary>Every live behaviour, by handle.</summary>
		public static List<Behaviour> Live()
		{
			return behaviours.OfType<Behaviour>().ToList();
		}

		/// <summary>The live behaviours among these handles, by execution order and then in the order given.</summary>
		public static List<Behaviour> Ordered(IEnumerable<uint> handles)
		{
			return handles.Select(Get).OfType<Behaviour>().OrderBy(behaviour => behaviour.Order).ToList();
		}

		/// <summary>
		/// Runs a phase's callbacks. While the editor's Profiler takes samples, each is a sample of its own, named for
		/// its class and method.
		/// </summary>
		public static void Invoke(Phase phase, ReadOnlySpan<uint> handles)
		{
			var sampled = Native.ProfilingEnabled;
			foreach (var handle in handles)
			{
				var behaviour = behaviours[(int)handle];
				if (behaviour == null || !ShouldRun(behaviour, phase))
				{
					continue;
				}

				if (sampled)
				{
					RunSampled(behaviour, phase);
				}
				else
				{
					Run(behaviour, phase);
				}
			}
		}

		public static void Release(ReadOnlySpan<uint> handles)
		{
			foreach (var handle in handles)
			{
				var behaviour = behaviours[(int)handle];
				if (behaviour == null)
				{
					continue;
				}

				InstanceRegistry.Unregister(behaviour);
				behaviour.Detach();
				behaviours[(int)handle] = null;
				free.Push((int)handle);
			}
		}

		/// <summary>
		/// Puts a behaviour of the same class from the scripts just loaded under each live handle, with the old one's
		/// fields carried over by transfer, and lets the old ones go. A behaviour whose class is not in the new build,
		/// or cannot be made, is let go and its handle freed. Returns the classes that were gone.
		/// </summary>
		public static List<string> Replace(StateTransfer transfer)
		{
			var gone = new List<string>();
			for (var handle = 0; handle < behaviours.Count; handle++)
			{
				if (behaviours[handle] is not { } previous)
				{
					continue;
				}

				var made = Remake(previous);
				behaviours[handle] = made;
				if (made != null)
				{
					transfer.Replaced[previous] = made;
					continue;
				}
				gone.Add(previous.GetType().FullName!);
				previous.Detach();
				free.Push(handle);
			}

			// Every replacement is made before any field is carried, so references between behaviours find theirs.
			foreach (var (previous, made) in transfer.Replaced)
			{
				Carry(transfer, previous, made);
			}
			return gone;
		}

		/// <summary>What the engine needs to schedule the behaviour under a handle, or the gone flag when there is none.</summary>
		public static BehaviourInfo Describe(uint handle)
		{
			var behaviour = Get(handle);
			var type = behaviour == null ? null : BehaviourTypes.Find(behaviour.GetType().FullName!);
			if (behaviour == null || type == null)
			{
				return new BehaviourInfo { Flags = BehaviourInfo.GoneFlag };
			}

			// One still waiting for its Awake has scene values to set then, as when it was made.
			var awake = behaviour.PendingFields == null ? 0 : 1u << (int)Phase.Awake;
			var flags = type.IsInstance ? BehaviourInfo.InstanceFlag : 0;
			return new BehaviourInfo { Callbacks = type.Callbacks | awake, Order = type.Order, Flags = flags };
		}

		private static Behaviour? Remake(Behaviour previous)
		{
			var type = BehaviourTypes.Find(previous.GetType().FullName!);
			if (type == null)
			{
				return null;
			}

			try
			{
				var made = type.Create();
				made.TakeOver(previous, type.Order);
				return made;
			}
			catch (Exception exception) when (exception is TargetInvocationException or MissingMethodException)
			{
				Log.Exception(exception.InnerException ?? exception, previous);
				return null;
			}
		}

		private static void Carry(StateTransfer transfer, Behaviour previous, Behaviour made)
		{
			try
			{
				transfer.Copy(previous, made, $"{made.GetType().Name} on {made.Obj.Name}");
			}
			catch (Exception exception)
			{
				Log.Exception(exception, made);
			}

			if (BehaviourTypes.Find(made.GetType().FullName!)?.IsInstance == true)
			{
				InstanceRegistry.Register(made);
			}
			previous.Detach();
		}

		private static int Allocate(Behaviour behaviour)
		{
			if (free.TryPop(out var handle))
			{
				behaviours[handle] = behaviour;
				return handle;
			}

			behaviours.Add(behaviour);
			return behaviours.Count - 1;
		}

		/// <summary>
		/// The engine applies enable and destroy requests between phases, so a behaviour that was
		/// disabled or destroyed earlier in this same pass is skipped here instead.
		/// </summary>
		private static bool ShouldRun(Behaviour behaviour, Phase phase)
		{
			var perFrame = phase == Phase.FixedUpdate || phase == Phase.Update || phase == Phase.LateUpdate;
			return !perFrame || (behaviour.Enabled && !behaviour.IsDestroyed);
		}

		private static void RunSampled(Behaviour behaviour, Phase phase)
		{
			var recorded = Native.ProfilerBegin(CallbackMarkers.For(behaviour, phase));
			Run(behaviour, phase);
			if (recorded)
			{
				Native.ProfilerEnd();
			}
		}

		public static void Run(Behaviour behaviour, Phase phase)
		{
			try
			{
				if (phase == Phase.Awake)
				{
					behaviour.ApplyFields();
				}
				behaviour.Dispatch(phase);
			}
			catch (Exception exception)
			{
				Log.Exception(exception, behaviour);
			}
		}
	}
}
