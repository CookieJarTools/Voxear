namespace Voxear
{
	/// <summary>Shows the fields of the group named in the Inspector as a row, the fields side by side.</summary>
	public sealed class HorizontalGroupAttribute : GroupAttribute
	{
		public HorizontalGroupAttribute(string name)
			: base(name)
		{
		}
	}
}
