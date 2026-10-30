using System.Runtime.InteropServices;

namespace Voxear
{
	/// <summary>
	/// What is loaded right now, from Content.Usage. Good for a debug overlay, or for checking that a level change
	/// freed what it should. Mirrors ContentStats in voxear_scripting.
	/// </summary>
	[StructLayout(LayoutKind.Sequential)]
	public readonly struct ContentUsage
	{
		public readonly uint Models;
		public readonly uint Prefabs;
		/// <summary>Prefabs held with Keep, which sweeps leave alone.</summary>
		public readonly uint KeptPrefabs;
		public readonly uint Materials;
		public readonly uint GpuMeshes;
		public readonly uint GpuMaterials;
		public readonly uint GpuTextures;
		/// <summary>Freed, and waiting for the frames still on the GPU to finish with them.</summary>
		public readonly uint GpuRetiring;

		public override string ToString()
		{
			return $"{Models} models, {Prefabs} prefabs ({KeptPrefabs} kept), {Materials} materials; GPU: {GpuMeshes} meshes, {GpuMaterials} materials, {GpuTextures} textures, {GpuRetiring} retiring";
		}
	}
}
