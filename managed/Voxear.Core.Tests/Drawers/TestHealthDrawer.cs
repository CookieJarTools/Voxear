using System;
using System.Collections.Generic;

namespace Voxear.Tests
{
	/// <summary>Heals by one each time it draws, and notes that it drew.</summary>
	internal sealed class TestHealthDrawer : ValueDrawer<TestHealth>
	{
		public static readonly List<string> Drawn = new List<string>();

		public override string Markup => "<vui><Label name=\"text\" /><Panel name=\"next\" /></vui>";

		protected override TestHealth Draw(TestHealth value, UiDocument ui)
		{
			Drawn.Add("health");
			value.Current = Math.Min(value.Current + 1, value.Max);
			return value;
		}
	}
}
