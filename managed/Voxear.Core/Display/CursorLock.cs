namespace Voxear
{
	/// <summary>How the mouse is held, as <see cref="Cursor.Lock"/> sets it.</summary>
	public enum CursorLock
	{
		/// <summary>Free to leave the window.</summary>
		None = 0,

		/// <summary>Held in place and hidden, so its moves turn a camera without limit, as a first-person game wants.</summary>
		Locked = 1,

		/// <summary>Kept inside the window, as a strategy game wants for scrolling at its edges.</summary>
		Confined = 2,
	}
}
