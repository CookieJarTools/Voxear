using System.Numerics;

namespace Voxear
{
	/// <summary>How Draw draws right now, as the scopes around a call have set it.</summary>
	internal struct DrawState
	{
		public Color Color;
		public Matrix4x4 Matrix;
		public float Width;
		public float Duration;
		public bool OnTop;
		public bool InGame;

		public static DrawState Default => new DrawState { Color = Color.White, Matrix = Matrix4x4.Identity };
	}
}
