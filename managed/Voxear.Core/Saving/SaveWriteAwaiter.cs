using System;
using System.Runtime.CompilerServices;

namespace Voxear
{
	/// <summary>Lets a SaveWrite be awaited. The await throws a SaveException when the write failed.</summary>
	public readonly struct SaveWriteAwaiter : INotifyCompletion
	{
		private readonly SaveWrite write;

		internal SaveWriteAwaiter(SaveWrite write)
		{
			this.write = write;
		}

		public bool IsCompleted => write.IsDone;

		public void OnCompleted(Action continuation)
		{
			write.OnCompleted(continuation);
		}

		public void GetResult()
		{
			if (write.Failure != null)
			{
				throw write.Failure;
			}
		}
	}
}
