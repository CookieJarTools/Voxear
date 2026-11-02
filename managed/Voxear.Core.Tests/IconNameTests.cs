namespace Voxear.Tests
{
	internal static class IconNameTests
	{
		[Test]
		private static void IconNamesAreWrittenAsVoxearUiNamesThem()
		{
			Check.True(IconNames.Of(IconName.Audio) == "audio", IconNames.Of(IconName.Audio));
			Check.True(IconNames.Of(IconName.ChevronDown) == "chevron-down", IconNames.Of(IconName.ChevronDown));
		}
	}
}
