namespace Voxear;

/// <summary>One of Voxear UI's icons, by its name, as "play" or "settings", coloured as its text is.</summary>
public class Icon : UiElement
{
	private readonly string name;

	public Icon(string name) : base(UiKind.Icon, string.Empty)
	{
		this.name = name;
	}

	public Icon(IconName name) : this(IconNames.Of(name))
	{
	}

	private protected override string MadeFrom => name;
}
