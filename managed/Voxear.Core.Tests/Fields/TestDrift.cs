using System.Numerics;

namespace Voxear.Tests
{
	/// <summary>A component, as a scene gives one to an Obj.</summary>
	internal struct TestDrift : IComponent
	{
		public Vector3 Velocity;

		[Range(0, 10)]
		public float Wobble;

		public Obj Anchor;
	}
}
