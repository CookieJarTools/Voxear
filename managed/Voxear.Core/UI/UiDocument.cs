using System.Text;
using Voxear.Interop;

namespace Voxear
{
	/// <summary>
	/// A UI the game shows over its view: a .vui document from the project, with the sheets its Style elements name,
	/// laid out by Voxear UI as the editor is. <c>var hud = UiDocument.Load("UI/Hud.vui");</c>, then
	/// <c>hud.Find("score").Text = "10";</c> and <c>hud.Find("pause").Clicked += Pause;</c>. It shows over the whole
	/// window in a built game and over the Game view in the editor, until it is closed or play mode stops.
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
}
