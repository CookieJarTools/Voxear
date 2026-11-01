using Voxear;

namespace Hello
{
	/// <summary>
	/// Commands for the editor's palette (Ctrl+K). They run on the level being edited, and what they change is one step
	/// to undo.
	/// </summary>
	internal static class Tools
	{
		/// <summary>Swaps where Spinner A and Spinner B stand.</summary>
		[Command("Swap Spinners", Keys = "Ctrl+Shift+X")]
		private static void SwapSpinners()
		{
			var a = Obj.Find("Spinner A");
			var b = Obj.Find("Spinner B");
			if (!a.IsValid || !b.IsValid)
			{
				Log.Warning("Swap Spinners needs Objs called Spinner A and Spinner B.");
				return;
			}

			(a.Position, b.Position) = (b.Position, a.Position);
			Log.Info("Swapped Spinner A and Spinner B.");
		}

		/// <summary>
		/// A check for VoxDocs' Step-by-Step Guides page: it passes once Spinner A stands to the right of Spinner B, as
		/// Swap Spinners leaves them, and says what is still to do until then.
		/// </summary>
		[DocsCheck("Spinner A stands on the right")]
		private static string? SpinnerAOnTheRight()
		{
			var a = Obj.Find("Spinner A");
			var b = Obj.Find("Spinner B");
			if (!a.IsValid || !b.IsValid)
			{
				return "Hello needs Objs called Spinner A and Spinner B.";
			}

			return a.Position.X > b.Position.X ? null : "Spinner A is on the left: swap them.";
		}
	}
}
