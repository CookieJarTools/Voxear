using System;

namespace Voxear
{
	/// <summary>
	/// How Draw draws until the end of a using block, which puts back what was there before:
	/// <c>using (Draw.Color(Color.Red)) { Draw.WireSphere(target, 1f); }</c>. Scopes nest, and each thread has its own.
	/// </summary>
	public readonly struct DrawScope : IDisposable
	{
		private readonly DrawState before;

		internal DrawScope(DrawState before)
		{
			this.before = before;
		}

		public void Dispose()
		{
			Draw.Restore(before);
		}
	}
}
