use std::io::Write;

use voxear_assets::AssetDb;
use voxear_engine::prefab::PrefabFile;

use super::*;

/// The fake Megascans download beside the tests: a boulder as Fab's FBX
/// downloads are laid out, with a model for each of two LODs, its maps as
/// tiny JPEGs, and a `.json`.
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/Fake_Boulder_xqzvbnd")
}

fn fixture_file(name: &str) -> Vec<u8> {
    std::fs::read(fixture().join(name)).unwrap()
}

/// An 8 × 8 JPEG of one colour.
fn jpg(color: [u8; 3]) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    RgbImage::from_pixel(8, 8, image::Rgb(color)).write_to(&mut bytes, image::ImageFormat::Jpeg).unwrap();
    bytes.into_inner()
}

fn gray(value: u8) -> Vec<u8> {
    jpg([value; 3])
}

fn png(image: DynamicImage) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}

fn zip(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in files {
        writer.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn write_files(root: &Path, files: &[(&str, Vec<u8>)]) {
    for (name, bytes) in files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
}

/// The names in a folder, in order.
fn names(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn material(path: &Path) -> MaterialData {
    toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn pixel(path: &Path, x: u32, y: u32) -> [u8; 4] {
    image::open(path).unwrap().to_rgba8().get_pixel(x, y).0
}

fn near(value: u8, expected: u8) -> bool {
    value.abs_diff(expected) <= 4
}

/// The project path of the asset a GUID names.
fn path_of(db: &AssetDb, guid: Option<AssetGuid>) -> Option<String> {
    Some(db.get(guid?)?.path.clone())
}

#[test]
fn pictures_and_models_are_read_by_their_names_as_fab_and_older_downloads_write_them() {
    let fab = Picture::read("Small_Rocks_Pack_ucdjdfmva_High_4K_BaseColor.jpg").unwrap();
    assert_eq!((fab.map, fab.size, fab.lod, fab.billboard), (Map::Color, 4, None, false));
    assert_eq!(fab.words, ["Small", "Rocks", "Pack", "ucdjdfmva", "High"]);
    assert_eq!(Picture::read("udlladln_8K_Albedo.jpg").unwrap().map, Map::Color);
    let baked = Picture::read("Aset_wood_log_M_rgzoN_2K_Normal_LOD1.jpg").unwrap();
    assert_eq!((baked.map, baked.size, baked.lod), (Map::Normal, 2, Some(1)));
    let packed = Picture::read("Textures/T_weflciqaa_8K_ORM.png").unwrap();
    assert_eq!(
        (packed.map, packed.size, packed.words.as_slice()),
        (Map::Packed, 8, ["weflciqaa".to_string()].as_slice())
    );
    assert_eq!(Picture::read("T_weflciqaa_1K_B.jpg").unwrap().map, Map::Color, "a glTF download's letters");
    assert_eq!(Picture::read("Mossy_Floor_vfylbge_4K_Gloss.jpg").unwrap().map, Map::Gloss);
    assert!(Picture::read("Billboard/Billboard_2K_Albedo.jpg").unwrap().billboard);
    assert!(!Picture::read("tjztdgxfa_2K_Displacement.exr").unwrap().readable());
    assert!(Picture::read("tjztdgxfa_2K_Bump.exr").is_none(), "a map no material reads");
    assert!(Picture::read("Brick_BaseColor.png").is_none(), "without a size, it is not named as Megascans names them");
    assert!(Picture::read("Rock_B.png").is_none(), "a letter names a map only after T_");

    let plant = ModelFile::read("Tufted_Grass_tdcnfcdr_High_tdcnfcdr_VarA_LOD2.fbx").unwrap();
    assert_eq!(
        (plant.lod, plant.variation.as_deref(), plant.tier, plant.billboard),
        (Some(2), Some("A"), Some(1), false)
    );
    let older = ModelFile::read("Var3/Var3_LOD0.fbx").unwrap();
    assert_eq!((older.lod, older.variation.as_deref()), (Some(0), Some("3")));
    let gltf = ModelFile::read("tdcnfcdr_tier_0.gltf").unwrap();
    assert!(gltf.gltf() && gltf.tier == Some(0));
    assert!(ModelFile::read("Billboard/Billboard.fbx").unwrap().billboard);
    assert!(ModelFile::read("tdcnfcdr_LOD0.obj").is_none(), "Voxear does not read OBJ");

    let named = |name: &str| name_words(&words(name)).join(" ");
    assert_eq!(named("Rock_Cliff_02_vdlnbfx_High"), "Rock Cliff 02 vdlnbfx", "a name keeps its number");
    assert_eq!(named("qwmzvpl_tier_1"), "qwmzvpl", "a tier's number is not the name's");
    assert_eq!(named("Tufted_Grass_tdcnfcdr_High_tdcnfcdr_VarA_LOD2"), "Tufted Grass tdcnfcdr tdcnfcdr");

    assert_eq!(metres("4 cm"), Some(0.04));
    assert_eq!(metres("0.05 m"), Some(0.05));
    assert_eq!(metres("50mm"), Some(0.05));
    assert_eq!(metres("2x2 m"), None);
}

#[test]
fn a_mesh_open_behind_is_told_from_a_closed_one() {
    let points = [Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z].to_vec();
    // A tetrahedron, each face wound to face out.
    let faces = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
    let closed = MeshTriangles { points: points.clone(), indices: faces.concat() };
    assert!(!open_behind(&closed), "a closed mesh's faces cancel out");
    let without_its_base = MeshTriangles { points: points.clone(), indices: faces[1..].concat() };
    assert!(open_behind(&without_its_base), "open where its base was");
    let card = MeshTriangles { points, indices: vec![0, 1, 2] };
    assert!(open_behind(&card), "a card is all front");
    assert!(!open_behind(&MeshTriangles::default()), "nothing is not open");
}

#[test]
fn a_model_with_a_file_for_each_lod_becomes_a_prefab_with_a_lod_group() {
    let project = tempfile::tempdir().unwrap();
    let into = project.path().join(FOLDER);
    assert!(is_download(&fixture()));
    let imported = import(&fixture(), &into).unwrap();
    assert_eq!(imported.folder, "Fake Boulder");
    assert_eq!(imported.select, "Fake Boulder/Fake Boulder.prefab");
    assert!(imported.skipped.is_empty(), "{:?}", imported.skipped);
    assert_eq!(names(&into), ["Fake Boulder"], "nothing left of the import but the asset");

    let asset = into.join("Fake Boulder");
    let files = names(&asset);
    let expected = [
        "Fake Boulder.material",
        "Fake Boulder.material.meta",
        "Fake Boulder.prefab",
        "Fake Boulder.prefab.meta",
        "Fake Boulder_LOD0.fbx",
        "Fake Boulder_LOD0.fbx.meta",
        "Fake Boulder_LOD1.fbx",
        "Fake Boulder_LOD1.fbx.meta",
        "Textures",
    ];
    assert_eq!(files, expected);
    let pictures = names(&asset.join("Textures"));
    let expected = ["Fake Boulder Colour.jpg", "Fake Boulder Metallic.png", "Fake Boulder Normal.jpg"];
    let expected: Vec<String> = expected.iter().flat_map(|name| [name.to_string(), format!("{name}.meta")]).collect();
    assert_eq!(pictures, expected, "a model's displacement is its shape: no height map");

    let (db, report) = AssetDb::scan(project.path()).unwrap();
    assert!(report.created.is_empty() && report.reassigned.is_empty(), "every file came with its .meta: {report:?}");
    let data = material(&asset.join("Fake Boulder.material"));
    let textures = "Megascans/Fake Boulder/Textures";
    assert_eq!(path_of(&db, data.color_map), Some(format!("{textures}/Fake Boulder Colour.jpg")));
    assert_eq!(path_of(&db, data.metallic_map), Some(format!("{textures}/Fake Boulder Metallic.png")));
    assert_eq!(path_of(&db, data.normal_map), Some(format!("{textures}/Fake Boulder Normal.jpg")));
    assert_eq!((data.height_map, data.displacement), (None, 0.0));
    assert_eq!((data.color, data.roughness, data.metallic), ([1.0; 3], 1.0, 0.0), "the maps as they are");
    assert!(!data.foliage && data.cutout.is_none());
    assert!(data.two_sided, "its models, a card and a triangle, are open behind, so drawn from both sides");

    let [occlusion, roughness, metalness, _] = pixel(&asset.join("Textures/Fake Boulder Metallic.png"), 3, 3);
    assert!(near(occlusion, 100) && near(roughness, 200) && metalness == 0, "{occlusion} {roughness} {metalness}");

    let material = db.guid_of("Megascans/Fake Boulder/Fake Boulder.material");
    let mut levels = Vec::new();
    for lod in 0..2 {
        let model = asset.join(format!("Fake Boulder_LOD{lod}.fbx"));
        assert!(!ModelImport::read(&model).lods, "the LOD Group's levels are its LODs");
        assert_eq!(MaterialRemap::read(&model).get("material0"), material, "drawn with the material");
        levels.push(db.guid_of(&format!("Megascans/Fake Boulder/Fake Boulder_LOD{lod}.fbx")).unwrap());
    }

    let text = std::fs::read_to_string(asset.join("Fake Boulder.prefab")).unwrap();
    let parsed: PrefabFile = toml::from_str(&text).unwrap();
    assert_eq!(
        (parsed.format, parsed.objs.len()),
        (voxear_engine::prefab::PREFAB_FORMAT, 3),
        "a prefab the engine reads"
    );
    let prefab: toml::Table = text.parse().unwrap();
    let objs = prefab["obj"].as_array().unwrap();
    assert_eq!(objs.len(), 3, "{text}");
    let group = objs[0]["lod_group"]["levels"].as_array().unwrap();
    let shown: Vec<i64> = group.iter().map(|level| level["objs"][0].as_integer().unwrap()).collect();
    assert_eq!(shown, [2, 3]);
    assert_eq!(objs[1]["name"].as_str(), Some("Fake Boulder_LOD0"));
    for (obj, model) in objs[1..].iter().zip(&levels) {
        assert_eq!(obj["parent"].as_integer(), Some(1));
        assert_eq!(obj["renderer"]["mesh"].as_str(), Some(format!("{model}#mesh0.0").as_str()), "{text}");
        assert_eq!(obj["renderer"]["material"].as_str(), Some(material.unwrap().to_string().as_str()));
    }
}

#[test]
fn a_zipped_plant_is_cut_out_by_its_opacity_and_sways() {
    let downloads = tempfile::tempdir().unwrap();
    let opacity = GrayImage::from_fn(8, 8, |x, _| image::Luma([if x < 4 { 0 } else { 255 }]));
    let (lod0, lod1) =
        (fixture_file("Fake_Boulder_xqzvbnd_High_LOD0.fbx"), fixture_file("Fake_Boulder_xqzvbnd_High_LOD1.fbx"));
    let download = downloads.path().join("Fake_Fern_pqlwxtv.zip");
    std::fs::write(
        &download,
        zip(&[
            ("Fake_Fern_pqlwxtv_High_4K_BaseColor.jpg", jpg([60, 140, 40])),
            ("Fake_Fern_pqlwxtv_High_4K_Opacity.png", png(DynamicImage::ImageLuma8(opacity))),
            ("Fake_Fern_pqlwxtv_High_4K_Normal.jpg", jpg([128, 128, 255])),
            ("Fake_Fern_pqlwxtv_High_4K_Roughness.jpg", gray(180)),
            ("Fake_Fern_pqlwxtv_High_pqlwxtv_VarA_LOD0.fbx", lod0.clone()),
            ("Fake_Fern_pqlwxtv_High_pqlwxtv_VarA_LOD1.fbx", lod1),
            ("Fake_Fern_pqlwxtv_High_pqlwxtv_VarB_LOD0.fbx", lod0),
        ]),
    )
    .unwrap();
    let project = tempfile::tempdir().unwrap();
    let into = project.path().join(FOLDER);
    assert!(is_download(&download));
    let imported = import(&download, &into).unwrap();
    assert_eq!((imported.folder.as_str(), imported.select.as_str()), ("Fake Fern", "Fake Fern/Fake Fern A.prefab"));

    let asset = into.join("Fake Fern");
    let files: Vec<String> = names(&asset).into_iter().filter(|name| !name.ends_with(".meta")).collect();
    let expected = [
        "Fake Fern A.prefab",
        "Fake Fern A_LOD0.fbx",
        "Fake Fern A_LOD1.fbx",
        "Fake Fern B.fbx",
        "Fake Fern.material",
        "Textures",
    ];
    assert_eq!(files, expected, "a variation with one model is dragged in as it is");

    let data = material(&asset.join("Fake Fern.material"));
    assert!(data.foliage, "lit as leaves");
    assert_eq!(data.cutout, Some(CUTOFF), "cut out by its opacity");
    assert!(data.two_sided, "its leaves seen from both sides");
    let colour = asset.join("Textures/Fake Fern Colour.png");
    let ([.., clear], [r, g, b, solid]) = (pixel(&colour, 1, 1), pixel(&colour, 6, 1));
    assert!(near(clear, 0) && near(solid, 255), "the opacity is the colour's alpha: {clear} {solid}");
    assert!(near(r, 60) && near(g, 140) && near(b, 40));

    let single = ModelImport::read(&asset.join("Fake Fern B.fbx"));
    assert_eq!(single.wind_masks, WindMasks::Generate, "its leaves flutter apart from its stem");
    assert!(single.lods, "drawn with the engine's LODs, as it has none of its own");
    assert!(!ModelImport::read(&asset.join("Fake Fern A_LOD0.fbx")).lods);
}

#[test]
fn a_surface_turns_gloss_to_roughness_and_keeps_its_height() {
    let downloads = tempfile::tempdir().unwrap();
    let download = downloads.path().join("vmqkzhr_2K_surface_ms");
    let json = r#"{ "id": "vmqkzhr", "name": "Fake Gravel", "semanticTags": { "asset_type": "surface" },
        "meta": [{ "key": "height", "name": "Height", "value": "4 cm" }] }"#;
    write_files(
        &download,
        &[
            ("vmqkzhr_2K_Albedo.jpg", jpg([120, 110, 100])),
            ("vmqkzhr_2K_Normal.jpg", jpg([128, 128, 255])),
            ("vmqkzhr_2K_Gloss.jpg", gray(60)),
            ("vmqkzhr_2K_Displacement.jpg", gray(128)),
            ("vmqkzhr_2K_Displacement.exr", b"not read".to_vec()),
            ("vmqkzhr_2K_Cavity.jpg", gray(200)),
            ("vmqkzhr.json", json.as_bytes().to_vec()),
        ],
    );
    let project = tempfile::tempdir().unwrap();
    let into = project.path().join(FOLDER);
    let imported = import(&download, &into).unwrap();
    assert_eq!(
        (imported.folder.as_str(), imported.select.as_str()),
        ("Fake Gravel", "Fake Gravel/Fake Gravel.material")
    );

    let asset = into.join("Fake Gravel");
    let pictures: Vec<String> =
        names(&asset.join("Textures")).into_iter().filter(|name| !name.ends_with(".meta")).collect();
    let expected =
        ["Fake Gravel Colour.jpg", "Fake Gravel Height.jpg", "Fake Gravel Metallic.png", "Fake Gravel Normal.jpg"];
    assert_eq!(pictures, expected, "what no material reads is left out");
    let data = material(&asset.join("Fake Gravel.material"));
    assert!(data.height_map.is_some());
    assert!((data.displacement - 0.04).abs() < 1e-6, "as deep as the scan: {}", data.displacement);
    let [occlusion, roughness, metalness, _] = pixel(&asset.join("Textures/Fake Gravel Metallic.png"), 3, 3);
    assert!(occlusion == 255 && near(roughness, 195) && metalness == 0, "{occlusion} {roughness} {metalness}");
}

/// A glTF of one triangle, reading its buffer and three pictures, as a
/// glTF download's model does.
fn triangle_gltf(id: &str) -> (Vec<u8>, Vec<u8>) {
    let mut buffer = Vec::new();
    for value in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0] {
        buffer.extend_from_slice(&value.to_le_bytes());
    }
    let gltf = serde_json::json!({
        "asset": { "version": "2.0" },
        "buffers": [{ "uri": format!("{id}_tier_1.bin"), "byteLength": 60 }],
        "bufferViews": [{ "buffer": 0, "byteOffset": 0, "byteLength": 36 }, { "buffer": 0, "byteOffset": 36, "byteLength": 24 }],
        "accessors": [
            { "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 0.0] },
            { "bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC2" }
        ],
        "images": [
            { "uri": format!("Textures/T_{id}_1K_B.png") },
            { "uri": format!("Textures/T_{id}_1K_N.png") },
            { "uri": format!("Textures/T_{id}_1K_ORM.png") }
        ],
        "textures": [{ "source": 0 }, { "source": 1 }, { "source": 2 }],
        "materials": [{
            "pbrMetallicRoughness": { "baseColorTexture": { "index": 0 }, "metallicRoughnessTexture": { "index": 2 } },
            "normalTexture": { "index": 1 }
        }],
        "meshes": [{ "primitives": [{ "attributes": { "POSITION": 0, "TEXCOORD_0": 1 }, "material": 0 }] }],
        "nodes": [{ "mesh": 0 }],
        "scenes": [{ "nodes": [0] }],
        "scene": 0
    });
    (serde_json::to_vec(&gltf).unwrap(), buffer)
}

#[test]
fn a_gltf_download_is_read_from_its_best_zip_with_its_packed_map_and_its_links() {
    let (gltf, buffer) = triangle_gltf("qwmzvpl");
    let picture = |color: [u8; 3]| png(DynamicImage::ImageRgb8(RgbImage::from_pixel(4, 4, image::Rgb(color))));
    let orm = picture([255, 90, 30]);
    let high = zip(&[
        ("qwmzvpl_tier_1/qwmzvpl_tier_1.gltf", gltf),
        ("qwmzvpl_tier_1/qwmzvpl_tier_1.bin", buffer),
        ("qwmzvpl_tier_1/Textures/T_qwmzvpl_1K_B.png", picture([90, 120, 60])),
        ("qwmzvpl_tier_1/Textures/T_qwmzvpl_1K_N.png", picture([128, 128, 255])),
        ("qwmzvpl_tier_1/Textures/T_qwmzvpl_1K_ORM.png", orm.clone()),
    ]);
    let low = zip(&[("Textures/T_qwmzvpl_1K_B.png", picture([0, 0, 0]))]);
    let downloads = tempfile::tempdir().unwrap();
    let download = downloads.path().join("Fake_Moss_qwmzvpl.zip");
    std::fs::write(&download, zip(&[("low.zip", low), ("high.zip", high)])).unwrap();

    let project = tempfile::tempdir().unwrap();
    let into = project.path().join(FOLDER);
    let imported = import(&download, &into).unwrap();
    assert_eq!((imported.folder.as_str(), imported.select.as_str()), ("Fake Moss", "Fake Moss/Fake Moss.gltf"));

    let asset = into.join("Fake Moss");
    assert_eq!(
        std::fs::read(asset.join("Textures/Fake Moss Metallic.png")).unwrap(),
        orm,
        "a packed map is taken as it is"
    );
    let written: Value = serde_json::from_slice(&std::fs::read(asset.join("Fake Moss.gltf")).unwrap()).unwrap();
    assert_eq!(written["buffers"][0]["uri"], "Fake%20Moss.bin");
    let images: Vec<&str> =
        written["images"].as_array().unwrap().iter().map(|image| image["uri"].as_str().unwrap()).collect();
    let expected = [
        "Textures/Fake%20Moss%20Colour.png",
        "Textures/Fake%20Moss%20Normal.png",
        "Textures/Fake%20Moss%20Metallic.png",
    ];
    assert_eq!(images, expected, "the glTF reads the pictures the material does");

    let data = material(&asset.join("Fake Moss.material"));
    assert_eq!((data.roughness, data.metallic), (1.0, 1.0), "both from the packed map");
    let (db, _) = AssetDb::scan(project.path()).unwrap();
    let model = "Megascans/Fake Moss/Fake Moss.gltf";
    let remap = MaterialRemap::read(&project.path().join(model));
    assert_eq!(remap.get("material0"), db.guid_of("Megascans/Fake Moss/Fake Moss.material"));
    let mut read = |file: &str| -> Result<Vec<u8>> { Ok(std::fs::read(project.path().join(file))?) };
    let prepared = voxear_render::prepare_model(model, &mut read).unwrap();
    assert_eq!(prepared.parts().count(), 1, "the model reads where its buffer and pictures are now");
}

#[test]
fn what_is_not_named_as_megascans_names_its_pictures_is_left_to_be_copied() {
    let downloads = tempfile::tempdir().unwrap();
    let files = [
        ("Brick.png", jpg([200, 80, 60])),
        ("notes.txt", b"mine".to_vec()),
        ("Crate.fbx", fixture_file("Fake_Boulder_xqzvbnd_High_LOD0.fbx")),
    ];
    let folder = downloads.path().join("Mine");
    write_files(&folder, &files);
    let zipped = downloads.path().join("Mine.zip");
    std::fs::write(&zipped, zip(&files)).unwrap();
    assert!(!is_download(&folder));
    assert!(!is_download(&zipped));
    assert!(!is_download(&folder.join("Brick.png")), "a picture on its own is a picture");
    assert!(!is_download(&downloads.path().join("Missing.zip")));

    let project = tempfile::tempdir().unwrap();
    let obj = downloads.path().join("Fake_Log_tbvkdw");
    write_files(
        &obj,
        &[("Fake_Log_tbvkdw_2K_BaseColor.jpg", jpg([90, 60, 40])), ("Fake_Log_tbvkdw_LOD0.obj", b"v 0 0 0".to_vec())],
    );
    let imported = import(&obj, project.path()).unwrap();
    assert_eq!(imported.select, "Fake Log/Fake Log.material", "its material, without the model it cannot read");
    assert_eq!(imported.skipped, ["Fake_Log_tbvkdw_LOD0.obj: Voxear reads FBX and glTF models"]);
}

#[test]
fn a_failed_import_leaves_nothing_behind_and_a_second_gets_a_folder_of_its_own() {
    let downloads = tempfile::tempdir().unwrap();
    let broken = downloads.path().join("Broken_rqzmvwx");
    write_files(
        &broken,
        &[("Broken_rqzmvwx_2K_BaseColor.jpg", b"not a picture".to_vec()), ("Broken_rqzmvwx_2K_Opacity.jpg", gray(255))],
    );
    let project = tempfile::tempdir().unwrap();
    let into = project.path().join(FOLDER);
    assert!(import(&broken, &into).is_err());
    assert!(names(&into).is_empty(), "{:?}", names(&into));

    import(&fixture(), &into).unwrap();
    let again = import(&fixture(), &into).unwrap();
    assert_eq!(
        (again.folder.as_str(), again.select.as_str()),
        ("Fake Boulder 1", "Fake Boulder 1/Fake Boulder.prefab")
    );
    assert_eq!(names(&into), ["Fake Boulder", "Fake Boulder 1"]);
    let (_, report) = AssetDb::scan(project.path()).unwrap();
    assert!(report.reassigned.is_empty(), "each import's GUIDs are its own");
}
