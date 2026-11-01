using System;

namespace Voxear.Tests
{
	/// <summary>A format of the game's own, found by itself: JSON written backwards.</summary>
	internal sealed class TestReversedFormat : SaveFormat
	{
		public override string Name => "reversed";

		public override byte[] Write(SaveNode root)
		{
			var bytes = Json.Write(root);
			Array.Reverse(bytes);
			return bytes;
		}

		public override SaveNode Read(ReadOnlySpan<byte> bytes)
		{
			var forwards = bytes.ToArray();
			Array.Reverse(forwards);
			return Json.Read(forwards);
		}
	}
}
