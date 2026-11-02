using System.Runtime.CompilerServices;

namespace Voxear;

/// <summary>The components a query hands out, kept inline so building a query allocates nothing.</summary>
[InlineArray(EntityQuery.MaxFetched)]
internal struct ComponentIds
{
	private uint first;
}
