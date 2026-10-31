namespace Voxear.Tests.Reloading
{
	/// <summary>A class of the scripts' own that can point at itself, so a reload has a circle to keep.</summary>
	public sealed class Crate
	{
		public string Label = string.Empty;
		public Crate? Next;
	}
}
