using System;

namespace Voxear.Saving
{
	/// <summary>Runs its action once when disposed, or nothing for a scope that only joined an outer one.</summary>
	internal sealed class Scope : IDisposable
	{
		private Action? end;

		public Scope(Action? end)
		{
			this.end = end;
		}

		public void Dispose()
		{
			end?.Invoke();
			end = null;
		}
	}
}
