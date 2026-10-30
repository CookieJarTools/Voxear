using System;
using System.Collections.Generic;
using System.Linq;
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

		/// <summary>The live behaviours among these handles, by execution order and then in the order given.</summary>
		public static List<Behaviour> Ordered(IEnumerable<uint> handles)
		{
			return handles.Select(Get).OfType<Behaviour>().OrderBy(behaviour => behaviour.Order).ToList();
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
