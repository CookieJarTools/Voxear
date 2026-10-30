using System;
using System.Collections.Generic;

namespace Voxear.Scripting
{
	/// <summary>The background loads scripts started, checked once per frame until they finish.</summary>
	internal static class Loading
	{
		private static readonly List<AsyncLoad> pending = new List<AsyncLoad>();

		public static T Track<T>(T load) where T : AsyncLoad
		{
			if (!load.IsDone)
			{
				pending.Add(load);
			}
			return load;
		}

		/// <summary>Finished loads resume their awaits here, which may start new loads, so a copy is walked.</summary>
		public static void Update()
		{
			var current = pending.ToArray();
			pending.Clear();
			foreach (var load in current)
			{
				if (!Poll(load))
				{
					pending.Add(load);
				}
			}
		}

		private static bool Poll(AsyncLoad load)
		{
			try
			{
				return load.Poll();
			}
			catch (Exception exception)
			{
				Log.Exception(exception);
				return true;
			}
		}
	}
}
