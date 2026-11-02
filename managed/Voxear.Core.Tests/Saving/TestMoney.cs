namespace Voxear.Tests;

/// <summary>A value with nothing public to save, kept as text by TestMoneyConverter.</summary>
internal readonly struct TestMoney
{
	private readonly long cents;

	public TestMoney(long cents)
	{
		this.cents = cents;
	}

	public long Cents => cents;
}
