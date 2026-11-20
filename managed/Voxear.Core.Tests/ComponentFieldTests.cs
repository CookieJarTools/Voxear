namespace Voxear.Tests;

/// <summary>The typed values component fields take, with no engine behind them.</summary>
internal static class ComponentFieldTests
{
	[Test]
	private static void ParticleTexturesAndMeshesAreBuiltInOrTheProjects()
	{
		Check.True(ParticleTexture.Puff.IsBuiltin && ParticleTexture.Puff.ToString() == "builtin:puff", "a built-in texture");
		Check.True(ParticleTexture.FromKey(null) == ParticleTexture.Dot, "a dot by default");
		var picture = ParticleTexture.Of(Texture.Load("Effects/Smoke.png"));
		Check.True(!picture.IsBuiltin && picture.ToString() == "Effects/Smoke.png", "a picture, by its path away from the engine");
		Check.True(!ParticleMesh.None.IsSet && ParticleMesh.Chunk.ToString() == "builtin:chunk", "built-in meshes");
		Check.True(ParticleMesh.FromKey("") == ParticleMesh.None, "none");
	}

	[Test]
	private static void AMeshIsBuiltInOrAModelsMesh()
	{
		Check.True(MeshAsset.Sphere.ToString() == "builtin:sphere", "a built-in mesh");
		var hull = MeshAsset.Of(ModelAsset.Load("Ships/Hull.gltf"), 2);
		Check.True(hull.ToString() == "Ships/Hull.gltf#mesh2.0", hull.ToString());
		Check.True(!MeshAsset.Of(default).IsSet, "a model not set names no mesh");
	}

	[Test]
	private static void ComponentsAreNoneAwayFromTheEngine()
	{
		var obj = new Obj(42);
		Check.True(obj.NavArea is null && obj.NavLink is null && obj.LodGroup is null && obj.FoliageRenderer is null, "none");
		Check.True(obj.PostVolume is null && obj.WindZone is null && obj.Ocean is null && obj.Pool is null, "none");
		Check.True(!Climate.Exists && Storms.Seed == 0u, "no level settings");
	}
}
