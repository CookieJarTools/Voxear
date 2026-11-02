using System;
using System.Collections.Generic;

namespace Voxear.Scripting
{
	/// <summary>
	/// A field the Inspector draws with drawers: the type of its value, its drawers, outermost first, and the Obj it
	/// is on. A whole behaviour, drawn by its inspector, has the behaviour's type.
	/// </summary>
	internal sealed class DrawnField
	{
		public DrawnField(Type valueType, List<Drawer> drawers, Obj obj)
		{
			ValueType = valueType;
			Drawers = drawers;
			Obj = obj;
		}

		public Type ValueType { get; }

		public List<Drawer> Drawers { get; }

		public Obj Obj { get; }

		/// <summary>Whether it is a whole behaviour, drawn by its inspector, rather than one of its fields.</summary>
		public bool IsBehaviour => Drawers.Count == 1 && DrawerTable.Inspects(Drawers[0].GetType()) != null;
	}
}
