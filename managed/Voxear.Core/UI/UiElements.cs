using System;
using System.Collections.Generic;
using Voxear.Interop;

namespace Voxear
{
	/// <summary>
	/// The UiElements scripts hold, one for each element, so a handler added to one is there the next time it is
	/// found, and an element made in code is the one found by its number; and the ones with Clicked or Changed
	/// handlers, which hear of their clicks and changes at the start of each frame. Elements are numbers in the world's
	/// game, so they are let go when the world changes, as each time play mode starts, and handlers are let go when the
	/// scripts reload, since they are the old scripts' code.
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

		/// <summary>An element made in code, now on screen with its number.</summary>
		public static void Adopt(UiElement element)
		{
			Settle();
			elements[element.Id] = element;
		}

		/// <summary>An element taken off the screen, whose number stands for nothing now.</summary>
		public static void Drop(UiElement element)
		{
			elements.Remove(element.Id);
		}

		public static void Listen(UiElement element)
		{
			if (!listening.Contains(element))
			{
				listening.Add(element);
			}
		}

		/// <summary>Raises Clicked and Changed on each element clicked or changed since the last frame.</summary>
		public static void Dispatch()
		{
			Settle();
			listening.RemoveAll(element => !element.HasListeners);
			foreach (var element in listening.ToArray())
			{
				DispatchOne(element);
			}
		}

		/// <summary>Raises one element's Clicked and Changed, if it was clicked or changed. Each handler is guarded on its own.</summary>
		public static void DispatchOne(UiElement element)
		{
			if (!element.IsShown)
			{
				return;
			}

			Guarded(element.WasChanged, element.RaiseChanged);
			Guarded(element.WasClicked, element.RaiseClicked);
		}

		/// <summary>Lets go of every element and handler, as when the scripts reload.</summary>
		public static void Forget()
		{
			elements.Clear();
			listening.Clear();
		}

		private static void Guarded(bool happened, Action raise)
		{
			if (!happened)
			{
				return;
			}

			try
			{
				raise();
			}
			catch (Exception exception)
			{
				Log.Exception(exception);
			}
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
