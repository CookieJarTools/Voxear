namespace Voxear.Tests
{
	/// <summary>A struct versioned apart from the type holding it: version 1 kept Slots as "size".</summary>
	[SaveVersion(2)]
	internal struct TestBag
	{
		public int Slots;

		[MigrateFrom(1)]
		private static void RenameSize(SaveNode bag)
		{
			bag.Rename("size", "Slots");
		}
	}
}
