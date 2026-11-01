namespace Voxear
{
	/// <summary>
	/// What a log line is about: an Obj, a behaviour's Obj, or an asset such as a prefab. Clicking the line in the
	/// Console shows it, in the Hierarchy or the Project panel, as Unity's log context does. Pass any of those where a
	/// log call takes one: <c>Log.Info("The door opened", door)</c>.
	/// </summary>
	public readonly struct LogContext
	{
		private LogContext(ulong obj, int prefab, string? asset)
		{
			Obj = obj;
			Prefab = prefab;
			Asset = asset;
		}

		/// <summary>The Obj, as its handle, or 0.</summary>
		internal ulong Obj { get; }

		/// <summary>The loaded prefab, one more than its engine id, or 0.</summary>
		internal int Prefab { get; }

		/// <summary>An asset's project path or GUID, as a reference names it.</summary>
		internal string? Asset { get; }

		public static implicit operator LogContext(Obj obj)
		{
			return new LogContext(obj.Id, 0, null);
		}

		public static implicit operator LogContext(Behaviour? behaviour)
		{
			return behaviour == null ? default : new LogContext(behaviour.Obj.Id, 0, null);
		}

		public static implicit operator LogContext(Prefab prefab)
		{
			return new LogContext(0, prefab.IsValid ? prefab.EngineId + 1 : 0, null);
		}

		public static implicit operator LogContext(Ref<Prefab> asset)
		{
			return new LogContext(0, 0, asset.IsSet ? asset.Path : null);
		}
	}
}
