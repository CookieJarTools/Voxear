using System;
using System.Collections.Generic;
using System.Text.Json;

namespace Voxear;

/// <summary>
/// The list and tree views on screen, by number, and the rows the engine has them make and fill each frame, after
/// the scripts' updates and before the frame is drawn: { "action": "ui-rows", "rows": [{ "list", "row", "made",
/// "item" }] }. Each row's code is guarded on its own, so one that throws leaves the rest filled.
/// </summary>
internal static class UiLists
{
	private static readonly Dictionary<int, CollectionView> shown = new Dictionary<int, CollectionView>();

	public static void Shown(CollectionView view)
	{
		shown[view.Id] = view;
	}

	public static void TakenOff(CollectionView view)
	{
		shown.Remove(view.Id);
	}

	/// <summary>Lets go of every view, as when the world changes.</summary>
	public static void Forget()
	{
		shown.Clear();
	}

	public static void Fill(JsonElement request)
	{
		foreach (var asked in request.GetProperty("rows").EnumerateArray())
		{
			if (shown.TryGetValue(asked.GetProperty("list").GetInt32(), out var view))
			{
				FillOne(view, asked);
			}
		}
	}

	private static void FillOne(CollectionView view, JsonElement asked)
	{
		var row = UiElements.Get(asked.GetProperty("row").GetInt32());
		try
		{
			if (asked.GetProperty("made").GetBoolean())
			{
				view.Make(row);
			}
			if (asked.GetProperty("item") is { ValueKind: JsonValueKind.Number } item)
			{
				view.Bind(row, item.GetUInt64());
			}
		}
		catch (Exception exception)
		{
			Log.Exception(exception);
		}
	}
}
