using System.Text;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A UI the game shows over its view, laid out by Voxear UI as the editor is: a .vui document from the project, with
/// the sheets its Style elements name, or one made empty and filled in code. <c>var hud = UiDocument.Load("UI/Hud.vui");</c>,
/// then <c>hud.Find("score").Text = "10";</c> and <c>hud.Find("pause").Clicked += Pause;</c>; or
/// <c>var hud = UiDocument.Create();</c> and <c>hud.Root.Add(new Button("Pause")).Clicked += Pause;</c>. It shows
/// over the whole window in a built game and over the Game view in the editor, until it is closed or play mode
/// stops.
/// </summary>
public sealed unsafe class UiDocument
{
	private readonly int id;

	internal UiDocument(int id)
	{
		this.id = id;
	}

	/// <summary>Whether it still shows: it has not been closed.</summary>
	public bool IsOpen { get; private set; } = true;

	/// <summary>The document's top element, around everything in it.</summary>
	public UiElement Root => Find(string.Empty)!;

	/// <summary>The number the engine holds it by, which a document loaded again keeps.</summary>
	internal int Id => id;

	/// <summary>Shows a document by project path. Null when it cannot be read, which the Console says why.</summary>
	public static UiDocument? Load(string path)
	{
		if (!Native.IsConnected)
		{
			return null;
		}

		var bytes = Encoding.UTF8.GetBytes(path);
		int id;
		fixed (byte* text = bytes)
		{
			id = Native.Api->UiLoad(text, bytes.Length);
		}
		return id < 0 ? null : new UiDocument(id);
	}

	/// <summary>
	/// An empty document over the game, to fill with elements made in code: <c>var hud = UiDocument.Create();</c>,
	/// then <c>hud.Root.Add(new Label("0"))</c>. Null when there is no UI to show it in, which the Console says.
	/// </summary>
	public static UiDocument? Create()
	{
		if (!Native.IsConnected)
		{
			return null;
		}

		var id = Native.Api->UiNew();
		return id < 0 ? null : new UiDocument(id);
	}

	/// <summary>The element with this name, or null. The same element comes back each time.</summary>
	public UiElement? Find(string name)
	{
		if (!IsOpen || !Native.IsConnected)
		{
			return null;
		}

		var bytes = Encoding.UTF8.GetBytes(name);
		int element;
		fixed (byte* text = bytes)
		{
			element = Native.Api->UiFind(id, text, bytes.Length);
		}
		return element < 0 ? null : UiElements.Get(element);
	}

	/// <summary>Takes it off the screen, with everything in it.</summary>
	public void Close()
	{
		if (IsOpen && Native.IsConnected)
		{
			Native.Api->UiClose(id);
		}
		IsOpen = false;
	}
}
