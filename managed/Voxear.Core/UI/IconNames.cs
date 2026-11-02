using System.Text;

namespace Voxear;

/// <summary>An <see cref="IconName"/> as Voxear UI names the icon: ChevronDown is "chevron-down".</summary>
internal static class IconNames
{
	public static string Of(IconName icon)
	{
		var name = icon.ToString();
		var written = new StringBuilder(name.Length + 4);
		for (var i = 0; i < name.Length; i++)
		{
			if (i > 0 && char.IsUpper(name[i]))
			{
				written.Append('-');
			}
			written.Append(char.ToLowerInvariant(name[i]));
		}
		return written.ToString();
	}
}
