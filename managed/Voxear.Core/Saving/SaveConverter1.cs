using System;

namespace Voxear;

/// <summary>
/// Saves T as the SaveNode Write returns, and makes a T again from it in Read. Neither sees null, which saves as
/// nothing by itself. Inside either, SaveNode.From and SaveNode.As save and read the values T holds as any other.
/// </summary>
public abstract class SaveConverter<T> : SaveConverter
{
	internal override Type Type => typeof(T);

	public abstract SaveNode Write(T value);

	public abstract T Read(SaveNode node);

	internal override SaveNode WriteObject(object value)
	{
		return Write((T)value);
	}

	internal override object? ReadObject(SaveNode node)
	{
		return Read(node);
	}
}
