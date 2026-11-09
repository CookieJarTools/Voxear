namespace Voxear;

/// <summary>What shows of what an element holds past its edges.</summary>
public enum Overflow
{
	/// <summary>All of it, the default.</summary>
	Visible,

	/// <summary>Nothing past its edges.</summary>
	Hidden,

	/// <summary>Nothing past its edges, scrolled to with the wheel.</summary>
	Scroll,
}
