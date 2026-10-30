using System;

namespace Voxear.Tests
{
	[Serializable]
	internal struct Wave
	{
		public int Count;

		[SerializeField] private float delay;

		public float Delay => delay;
	}
}
