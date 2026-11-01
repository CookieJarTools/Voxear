using System;

namespace Voxear
{
	/// <summary>
	/// Marks a static method that changes this type's saved values from one version to the next, before they are
	/// read: [MigrateFrom(1)] turns version 1 into version 2. It takes the type's SaveNode table and changes it in
	/// place. Migrations run one after another, so a save from version 1 read by version 3 goes through the methods for
	/// 1 and for 2. A version with nothing to change needs no method.
	/// </summary>
	[AttributeUsage(AttributeTargets.Method, AllowMultiple = false)]
	public sealed class MigrateFromAttribute : Attribute
	{
		public MigrateFromAttribute(int version)
		{
			Version = version;
		}

		public int Version { get; }
	}
}
