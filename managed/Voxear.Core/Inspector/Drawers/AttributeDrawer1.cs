using System;

namespace Voxear;

/// <summary>
/// Draws the fields marked with TAttribute, whatever their values are, as for a button beside the value that the
/// "next" element of its markup still shows. The value comes as an object, of the field's own type.
/// </summary>
public abstract class AttributeDrawer<TAttribute> : AttributeDrawer<TAttribute, object?>
	where TAttribute : Attribute
{
}
