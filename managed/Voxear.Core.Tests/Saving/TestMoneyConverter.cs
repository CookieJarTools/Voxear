using System.Globalization;

namespace Voxear.Tests
{
	/// <summary>Found by itself: saves money as "12.34".</summary>
	internal sealed class TestMoneyConverter : SaveConverter<TestMoney>
	{
		public override SaveNode Write(TestMoney value)
		{
			return (value.Cents / 100m).ToString("0.00", CultureInfo.InvariantCulture);
		}

		public override TestMoney Read(SaveNode node)
		{
			return new TestMoney((long)(decimal.Parse(node.AsString()!, CultureInfo.InvariantCulture) * 100));
		}
	}
}
