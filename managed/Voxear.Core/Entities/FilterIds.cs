using System.Runtime.CompilerServices;

namespace Voxear
{
	/// <summary>The components one of a query's filters names, kept inline as ComponentIds are.</summary>
	[InlineArray(EntityQuery.MaxFiltered)]
	internal struct FilterIds
	{
		private uint first;
	}
}
