using System;
using System.Reflection;

namespace Voxear.Saving;

/// <summary>A field or property a save keeps, by the name it keeps it under.</summary>
internal sealed class SaveMember
{
	private readonly FieldInfo? field;
	private readonly PropertyInfo? property;

	public SaveMember(string name, FieldInfo field)
	{
		Name = name;
		this.field = field;
		Type = field.FieldType;
	}

	public SaveMember(PropertyInfo property)
	{
		Name = property.Name;
		this.property = property;
		Type = property.PropertyType;
	}

	public string Name { get; }

	public Type Type { get; }

	public object? Get(object target)
	{
		return field != null ? field.GetValue(target) : property!.GetValue(target);
	}

	public void Set(object target, object? value)
	{
		if (field != null)
		{
			field.SetValue(target, value);
		}
		else
		{
			property!.SetValue(target, value);
		}
	}
}
