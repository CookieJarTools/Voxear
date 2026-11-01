using System;

namespace Voxear.Saving
{
	/// <summary>A converter made of two functions, for the engine's own types.</summary>
	internal sealed class DelegateConverter<T> : SaveConverter<T>
	{
		private readonly Func<T, SaveNode> write;
		private readonly Func<SaveNode, T> read;

		public DelegateConverter(Func<T, SaveNode> write, Func<SaveNode, T> read)
		{
			this.write = write;
			this.read = read;
		}

		public override SaveNode Write(T value)
		{
			return write(value);
		}

		public override T Read(SaveNode node)
		{
			return read(node);
		}
	}
}
