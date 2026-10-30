using System;
using System.Numerics;

namespace Voxear.Tests
{
	[Serializable]
	internal sealed class SpawnSettings
	{
		public Vector3 Offset;
		public Wave[] Waves = Array.Empty<Wave>();
	}
}
