using System;
using System.Collections.Generic;
using Voxear.Interop;

namespace Voxear
{
	/// <summary>
	/// The UiElements scripts hold, one for each element, so a handler added to one is there the next time it is
	/// found; and the ones with Clicked handlers, which hear of their clicks at the start of each frame. Elements are
	/// numbers in the world's game, so they are let go when the world changes, as each time play mode starts, and
	/// handlers are let go when the scripts reload, since they are the old scripts' code.
	/// </summary>
	internal static unsafe class UiElements
	{
		private static readonly Dictionary<int, UiElement> elements = new Dictionary<int, UiElement>();
		private static readonly List<UiElement> listening = new List<UiElement>();
		private static ulong world;

		public static UiElement Get(int id)
		{
			Settle();
			if (!elements.TryGetValue(id, out var element))
			{
				element = new UiElement(id);
				elements[id] = element;
			}
			return element;
		}

		public static void Listen(UiElement element)
		{
			if (!listening.Contains(element))
			{
				listening.Add(element);
			}
		}

		/// <summary>Raises Clicked on each element clicked since the last frame. Each handler is guarded on its own.</summary>
		public static void Dispatch()
		{
			Settle();
			listening.RemoveAll(element => !element.HasListeners);
			foreach (var element in listening.ToArray())
			{
				if (!element.WasClicked)
				{
					continue;
				}
				try
				{
					element.RaiseClicked();
				}
				catch (Exception exception)
				{
					Log.Exception(exception);
				}
			}
		}

		/// <summary>Lets go of every element and handler, as when the scripts reload.</summary>
		public static void Forget()
		{
			elements.Clear();
			listening.Clear();
		}

		/// <summary>Elements belong to the world they were found in; in another, they are let go.</summary>
		private static void Settle()
		{
			var current = Native.IsConnected ? *Native.Api->WorldId : 0;
			if (current != 0 && current != world)
			{
				Forget();
				world = current;
			}
		}
	}
}
