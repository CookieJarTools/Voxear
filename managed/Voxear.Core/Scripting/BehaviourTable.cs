using System;
using System.Collections.Generic;
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

		public static unsafe int Create(string typeName, Obj obj, BehaviourInfo* info)
		{
			var type = BehaviourTypes.Find(typeName);
			if (type == null)
			{
				Log.Error($"No Behaviour named {typeName} was found in the game scripts.");
				return -1;
			}

			var behaviour = type.Create();
			var handle = Allocate(behaviour);
			behaviour.Attach(obj, (uint)handle);
			info->Callbacks = type.Callbacks;
			info->Order = type.Order;
			info->Flags = type.IsInstance ? BehaviourInfo.InstanceFlag : 0;

			// Registered before any Awake runs, so every Awake in the batch can already reach it.
			if (type.IsInstance)
			{
				InstanceRegistry.Register(behaviour);
			}
			return handle;
		}

		public static void Invoke(Phase phase, ReadOnlySpan<uint> handles)
		{
			foreach (var handle in handles)
			{
				var behaviour = behaviours[(int)handle];
				if (behaviour != null && ShouldRun(behaviour, phase))
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

		private static void Run(Behaviour behaviour, Phase phase)
		{
			try
			{
				behaviour.Dispatch(phase);
			}
			catch (Exception exception)
			{
				Log.Exception(exception, behaviour);
			}
		}
	}
}
