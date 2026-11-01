using System;
using System.Collections.Generic;

namespace Voxear.Scripting
{
	/// <summary>A field the Inspector draws with drawers: the type of its value, and its drawers, outermost first.</summary>
	internal sealed class DrawnField
	{
		public DrawnField(Type valueType, List<Drawer> drawers)
		{
			ValueType = valueType;
			Drawers = drawers;
		}

		public Type ValueType { get; }

		public List<Drawer> Drawers { get; }
	}
}
