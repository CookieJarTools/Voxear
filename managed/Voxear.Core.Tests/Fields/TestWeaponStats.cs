using System.Collections.Generic;

namespace Voxear.Tests;

[CreateAsset("Weapons/Weapon Stats")]
internal sealed class TestWeaponStats : TestGearStats
{
	public float Damage = 10f;
	public List<int> Shots = new List<int>();
}
