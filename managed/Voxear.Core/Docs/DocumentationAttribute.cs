using System;

namespace Voxear;

/// <summary>
/// Links a behaviour, component, field, property or method to a VoxDocs page. The Inspector shows a book button
/// beside it that opens the page, and the page lists it under In the code.
/// <para>
/// Name the page by its path in the project's <c>Docs</c> folder, folders and all, as <c>"Combat/Damage"</c>, or by
/// the id in its header, which keeps working when the page is renamed or moved. Add a heading after a hash to open
/// the page at that section: <c>"Combat/Damage#falloff"</c>.
/// </para>
/// </summary>
[AttributeUsage(AttributeTargets.Class | AttributeTargets.Struct | AttributeTargets.Field | AttributeTargets.Property | AttributeTargets.Method, Inherited = true)]
public sealed class DocumentationAttribute : Attribute
{
	public DocumentationAttribute(string page)
	{
		Page = page;
	}

	/// <summary>The page, as its id or its path, and a heading after a hash if any.</summary>
	public string Page { get; }
}
