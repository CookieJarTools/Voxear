namespace Voxear;

/// <summary>A title that opens and closes what is added to it.</summary>
public class Foldout : UiElement
{
	public Foldout(string title = "") : base(UiKind.Foldout, title)
	{
	}
}
