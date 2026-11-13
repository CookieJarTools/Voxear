using System;
using Voxear.Interop;

namespace Voxear.Scripting;

/// <summary>Tells scripts of Heat Bodies' changes and characters' feelings, as HeatBody.Changed and HeatBody.FeelsChanged.</summary>
internal static class HeatMessages
{
	private const int Change = 0;

	public static void Dispatch(ReadOnlySpan<HeatEventData> events, string text)
	{
		var names = text.Split('\n');
		for (var at = 0; at < events.Length; at++)
		{
			var told = events[at];
			var obj = new Obj(told.Obj);
			if (told.Kind == Change)
			{
				var name = at < names.Length ? names[at] : string.Empty;
				HeatBody.Raise(new HeatChange(obj, told.Became == 0 ? Obj.None : new Obj(told.Became), name, told.Value));
				continue;
			}
			HeatBody.Raise(new HeatBody(obj), (Feels)told.Value);
		}
	}
}
