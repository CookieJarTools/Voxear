namespace Voxear;

/// <summary>
/// An amount of a currency, as a store's price, for a smart string to write as the player's language does:
/// "{price, currency}" is "€12.50" in English and "12,50 €" in French.
/// <code>
/// var text = Localization.Format("shop.price", ("price", new Money(12.5m, Currency.EUR)));
/// </code>
/// </summary>
public readonly record struct Money(decimal Amount, Currency Currency)
{
	/// <summary>As smart strings read it: the amount, then the currency's code.</summary>
	public override string ToString()
	{
		return $"{Amount.ToString(System.Globalization.CultureInfo.InvariantCulture)} {Currency}";
	}
}
