namespace Voxear;

/// <summary>
/// A picture of the project, as an item's icon in an inventory. Give it a size in its style: it shows its picture
/// over the whole of itself. With no picture it shows nothing but its background.
/// <code>
/// slot.Add(new Image(item.Icon)).AddClass("icon");
/// </code>
/// </summary>
public class Image : UiElement
{
	private const int StringPicture = 3;

	private Texture picture;

	public Image(Texture picture = default) : base(UiKind.Image, string.Empty)
	{
		this.picture = picture;
	}

	/// <summary>The picture it shows; None for nothing.</summary>
	public Texture Picture
	{
		get { return picture; }
		set
		{
			picture = value;
			SendString(StringPicture, value.Reference);
		}
	}

	private protected override void OnShown()
	{
		if (picture.IsSet)
		{
			SendString(StringPicture, picture.Reference);
		}
	}
}
