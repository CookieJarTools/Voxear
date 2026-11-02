namespace Voxear
{
	/// <summary>
	/// An engine component reached through its Obj, as an AudioSource is. A behaviour's field of one takes an Obj
	/// that has it: the Inspector offers only those, and a scene saves the Obj.
	/// </summary>
	internal interface IObjComponent
	{
		/// <summary>The Obj it belongs to.</summary>
		Obj Obj { get; }

		/// <summary>False when its Obj has no such component, or is gone.</summary>
		bool IsValid { get; }
	}
}
