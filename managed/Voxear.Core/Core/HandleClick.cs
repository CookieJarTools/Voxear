namespace Voxear
{
	/// <summary>The button the editor's pointer clicked, by the behaviour that showed it and which of its handles it is.</summary>
	internal readonly record struct HandleClick(ulong Entity, string Type, int Id);
}
