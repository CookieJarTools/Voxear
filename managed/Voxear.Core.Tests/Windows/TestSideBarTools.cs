namespace Voxear.Tests;

internal static class TestSideBarTools
{
	[SideBarButton(IconName.Grid, "Snap Everything")]
	private static void SnapEverything()
	{
	}

	[SideBarButton(IconName.Light, "Light Every Lamp", Menu = "Lamps")]
	private static void LightEveryLamp()
	{
	}

	[SideBarButton(IconName.Light, "Put Every Lamp Out", Menu = "Lamps")]
	private static void PutEveryLampOut()
	{
	}
}
