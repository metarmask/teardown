#![feature(array_map, array_chunks, stmt_expr_attributes)]
use std::{
    borrow::Cow,
    collections::{
        hash_map::{DefaultHasher, Entry},
        HashMap,
    },
    convert::{TryFrom, TryInto},
    error::Error,
    f32::consts::TAU,
    fs::{self, File},
    hash::{Hash, Hasher},
    io::{ErrorKind, Write},
    iter,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use derive_builder::Builder;
use nalgebra::{Isometry3, UnitQuaternion, Vector3};
pub(crate) use quick_xml::Result as XMLResult;
use quick_xml::{
    events::{BytesStart, Event},
    Writer,
};
use rayon::prelude::*;
use teardown_bin_format::{
    environment::{self, Fog, Skybox, Sun},
    Body, BoundaryVertex, Entity, EntityKind, EntityKindVariants, Environment, Exposure, Joint,
    JointKind, Light, LightKind, Material, MaterialKind, PaletteIndex, Rgba, Rope, Scene, Script,
    Sound, Transform, Vehicle, Voxels, Water,
};
use vox::semantic::{
    Material as VoxMaterial, MaterialKind as VoxMaterialKind, Model, Node, VoxFile, Voxel,
};

trait WriteXML {
    fn write_xml<W: Write>(&self, writer: &mut Writer<W>) -> XMLResult<()>;
}

trait ToXMLAttributes {
    fn to_xml_attrs(&self) -> Vec<(&'static str, String)>;
}

fn flatten_attrs(deep_attrs: Vec<Vec<(&'static str, String)>>) -> Vec<(&'static str, String)> {
    let mut flattened = Vec::new();
    for mut attrs in deep_attrs {
        flattened.append(&mut attrs);
    }
    flattened
}

impl ToXMLAttributes for Fog {
    fn to_xml_attrs(&self) -> Vec<(&'static str, String)> {
        vec![
            ("fogColor", join_as_strings(self.color.0.iter())),
            (
                "fogParams",
                join_as_strings(
                    [
                        self.start,
                        self.start + self.distance,
                        self.amount,
                        self.exponent,
                    ]
                    .iter(),
                ),
            ),
        ]
    }
}

impl ToXMLAttributes for Exposure {
    fn to_xml_attrs(&self) -> Vec<(&'static str, String)> {
        vec![
            ("exposure", join_as_strings([self.min, self.max].iter())),
            ("brightness", self.brightness_goal.to_string()),
        ]
    }
}

impl ToXMLAttributes for Skybox<'_> {
    fn to_xml_attrs(&self) -> Vec<(&'static str, String)> {
        flatten_attrs(vec![
            vec![
                (
                    "skybox",
                    PathBuf::from(self.texture)
                        .strip_prefix("data/env")
                        .map_or_else(|_| self.texture.to_string(), |x| x.display().to_string()),
                ),
                ("skyboxtint", join_as_strings(self.color_intensity.0.iter())),
                ("skyboxbright", 1.to_string()),
                ("skyboxrot", self.rotation.to_radians().to_string()),
                ("ambient", self.ambient_light.to_string()),
            ],
            self.sun.to_xml_attrs(),
        ])
    }
}

impl ToXMLAttributes for Sun {
    fn to_xml_attrs(&self) -> Vec<(&'static str, String)> {
        vec![
            ("sunBrightness", self.brightness.to_string()),
            ("sunColorTint", join_as_strings(self.tint.0.iter())),
            (
                "sunDir",
                "auto".to_string(), /* join_as_strings(self.direction.iter().map(|x|
                                     * x.to_degrees())) */
            ),
            ("sunSpread", self.spread.to_string()),
            ("sunLength", self.max_shadow_length.to_string()),
            ("sunFogScale", self.fog_scale.to_string()),
            ("sunGlare", self.glare.to_string()),
        ]
    }
}

impl ToXMLAttributes for environment::Water {
    fn to_xml_attrs(&self) -> Vec<(&'static str, String)> {
        vec![
            ("wetness", self.wetness.to_string()),
            ("puddleamount", self.puddle_coverage.to_string()),
            ("puddlesize", self.puddle_size.to_string()),
            ("rain", self.rain.to_string()),
        ]
    }
}

impl ToXMLAttributes for (&'static str, Sound<'_>) {
    fn to_xml_attrs(&self) -> Vec<(&'static str, String)> {
        vec![(
            self.0,
            join_as_strings([self.1.path, self.1.volume.to_string().as_ref()].iter()),
        )]
    }
}

impl<'a> WriteXML for Environment<'a> {
    fn write_xml<W: Write>(&self, writer: &mut Writer<W>) -> XMLResult<()> {
        writer.write_event(Event::Empty(
            BytesStart::borrowed_name("environment".as_bytes()).with_attributes(
                flatten_attrs(vec![
                    self.skybox.to_xml_attrs(),
                    self.exposure.to_xml_attrs(),
                    self.fog.to_xml_attrs(),
                    self.water.to_xml_attrs(),
                    vec![
                        ("nightlight", self.nightlight.to_string()),
                        (
                            "ambience",
                            join_as_strings(
                                [
                                    self.ambience.path,
                                    self.ambience.volume.to_string().as_ref(),
                                ]
                                .iter(),
                            ),
                        ),
                        ("slippery", self.slippery.to_string()),
                    ],
                    self.fog.to_xml_attrs(),
                ])
                .iter()
                .map(|(k, v)| (*k, v.as_ref())),
            ),
        ))?;
        Ok(())
    }
}

impl ToXMLAttributes for Transform {
    fn to_xml_attrs(&self) -> Vec<(&'static str, String)> {
        let mut attrs = Vec::new();
        let (pos, rot) = self.as_nalegbra_pair();
        attrs.push(("pos", join_as_strings(pos.iter())));
        attrs.push((
            "rot",
            join_as_strings({
                let (x, y, z) = rot.euler_angles();
                [x, y, z].map(|dim| dim.to_degrees()).iter()
            }),
        ));
        attrs
    }
}

impl ToXMLAttributes for Light<'_> {
    fn to_xml_attrs(&self) -> Vec<(&'static str, String)> {
        vec![
            (
                "type",
                match self.kind {
                    LightKind::Sphere => "sphere",
                    LightKind::Capsule => "capsule",
                    LightKind::Cone => "cone",
                    LightKind::Area => "area",
                }
                .to_string(),
            ),
            ("color", join_as_strings(self.rgba.0.iter())),
            ("scale", self.scale.to_string()),
            ("angle", self.cone_angle.to_degrees().to_string()),
            ("penumbra", self.cone_penumbra.to_degrees().to_string()),
            ("size", match self.kind {
                LightKind::Area => join_as_strings(self.area_size.iter().map(|half| half * 2.)),
                _ => self.size.to_string(),
            }),
            ("reach", self.reach.to_string()),
            ("unshadowed", self.unshadowed.to_string()),
            ("fogscale", self.fog_scale.to_string()),
            ("fogiter", self.fog_iter.to_string()),
            (
                "sound",
                join_as_strings([self.sound.path, self.sound.volume.to_string().as_ref()].iter()),
            ),
            ("glare", self.glare.to_string()),
        ]
    }
}

impl WriteXML for &[BoundaryVertex] {
    fn write_xml<W: Write>(&self, writer: &mut Writer<W>) -> XMLResult<()> {
        for BoundaryVertex { x, z } in *self {
            writer.write_event(Event::Empty(
                BytesStart::owned_name("vertex")
                    .with_attributes(vec![("pos", join_as_strings([x, z].iter()).as_ref())]),
            ))?;
        }
        Ok(())
    }
}

pub struct VoxStore {
    pub hash_vox_dir: PathBuf,
    pub palette_files: HashMap<u64, Arc<Mutex<VoxStoreFile>>>,
}

pub struct VoxStoreFile {
    path: PathBuf,
    vox: VoxFile,
    shape_indices: HashMap<u64, usize>,
    dirty: bool,
}

impl VoxStoreFile {
    fn new(path: PathBuf, palette: &[Material; 256]) -> Result<Self, Box<dyn Error + Send>> {
        let mut shape_indices = HashMap::new();
        let vox = if path.exists() {
            let file = vox::semantic::parse_file(&path)?;
            for (i, child) in file.root.children().unwrap_or(&vec![]).iter().enumerate() {
                if let Some(name) = &child.name {
                    if let Ok(hash_n) = hash_str_to_n(name) {
                        shape_indices.insert(hash_n, i);
                    }
                }
            }
            file
        } else {
            let mut file = VoxFile::new();
            file.set_palette(
                &palette
                    .iter()
                    .skip(1)
                    .map(convert_material)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            file
        };
        Ok(Self {
            dirty: false,
            vox,
            shape_indices,
            path,
        })
    }

    fn store_voxel_data(&mut self, voxel_data: &Voxels) {
        let hash_n = compute_hash_n(&voxel_data);
        match self.shape_indices.entry(hash_n) {
            Entry::Vacant(vacancy) => {
                let len = self.vox.root.children().map(Vec::len).unwrap_or_default();
                let mut voxels = Vec::new();
                for (coord, palette_index) in voxel_data.iter() {
                    if let Ok(pos) = coord
                        .iter()
                        .copied()
                        .map(TryInto::try_into)
                        .collect::<Result<Vec<_>, _>>()
                    {
                        voxels.push(Voxel {
                            pos: <[u8; 3]>::try_from(pos).unwrap(),
                            index: palette_index,
                        });
                    }
                }
                let model = Model::new(voxel_data.size.map(|dim| dim.min(256)), voxels);
                #[allow(clippy::cast_possible_wrap)]
                let [x, y, z] = model.size().map(|x| (x as i32) / 2);
                let mut node = Node::new([x, y - 1, z], model);
                node.name = Some(hash_n_to_str(hash_n));
                self.vox.root.add(node);
                self.dirty = true;
                vacancy.insert(len);
            }
            Entry::Occupied(_) => {}
        }
    }

    fn write(&mut self) -> Result<(), Box<dyn Error>> {
        self.vox.clone().write(&mut File::create(&self.path)?)?;
        self.dirty = false;
        Ok(())
    }

    fn write_dirty(&mut self) -> Result<(), Box<dyn Error>> {
        if self.dirty {
            self.write()
        } else {
            Ok(())
        }
    }
}

impl Drop for VoxStoreFile {
    fn drop(&mut self) {
        self.write_dirty().expect("while writing VoxStoreFile");
    }
}

impl VoxStore {
    pub fn new<P: AsRef<Path>>(teardown_dir: P) -> Result<Arc<Mutex<Self>>, String> {
        let vox_dir = teardown_dir.as_ref().join("data").join("vox");
        if !vox_dir.exists() {
            return Err("data/vox didn't exist in teardown_dir".into());
        }
        Ok(Arc::new(Mutex::new(Self {
            hash_vox_dir: vox_dir.join("hash"),
            palette_files: HashMap::new(),
        })))
    }

    fn load_palettes(&mut self, palettes: &[&[Material; 256]]) -> Vec<Arc<Mutex<VoxStoreFile>>> {
        let mut vec = Vec::new();
        for palette in palettes {
            let hash_n = compute_hash_n(palette);
            vec.push(match self.palette_files.entry(hash_n) {
                Entry::Occupied(occupant) => occupant.get().clone(),
                Entry::Vacant(vacancy) => vacancy
                    .insert(Arc::new(Mutex::new(
                        VoxStoreFile::new(
                            self.hash_vox_dir
                                .join(format!("{}.vox", hash_n_to_str(hash_n))),
                            palette,
                        )
                        .unwrap(),
                    )))
                    .clone(),
            })
        }
        vec
    }

    pub fn write_dirty(&mut self) -> Result<(), Box<dyn Error>> {
        for file in self.palette_files.values() {
            file.lock().unwrap().write_dirty()?;
        }
        Ok(())
    }
}

#[derive(Builder)]
pub struct SceneWriter<'a> {
    scene: &'a Scene<'a>,
    mod_dir: PathBuf,
    vox_store: Arc<Mutex<VoxStore>>,
    #[builder(default = "\"main\".into()")]
    name: String,
}

fn iter_material_kinds() -> impl Iterator<Item = MaterialKind> {
    [
        MaterialKind::None,
        MaterialKind::Glass,
        MaterialKind::Wood,
        MaterialKind::Masonry,
        MaterialKind::Plaster,
        MaterialKind::Metal,
        MaterialKind::HeavyMetal,
        MaterialKind::Rock,
        MaterialKind::Dirt,
        MaterialKind::Foliage,
        MaterialKind::Plastic,
        MaterialKind::HardMetal,
        MaterialKind::HardMasonry,
        MaterialKind::Unknown13,
        MaterialKind::Unphysical,
    ]
    .iter()
    .copied()
}

fn material_kind_for_index(index: u8) -> MaterialKind {
    for material_kind in iter_material_kinds() {
        let range = range_for_material_kind(material_kind);
        if index >= range[0] && index <= range[1] {
            return material_kind;
        }
    }
    MaterialKind::None
}

fn range_for_material_kind(material_kind: MaterialKind) -> [u8; 2] {
    match material_kind {
        MaterialKind::Glass => [1, 8],
        MaterialKind::Foliage => [9, 24],
        MaterialKind::Dirt => [25, 40],
        MaterialKind::Rock => [41, 56],
        MaterialKind::Wood => [57, 72],
        MaterialKind::Masonry => [73, 104],
        MaterialKind::Plaster => [105, 120],
        MaterialKind::Metal => [121, 136],
        MaterialKind::HeavyMetal => [137, 152],
        MaterialKind::Plastic => [153, 168],
        MaterialKind::HardMetal => [169, 152],
        MaterialKind::HardMasonry => [177, 184],
        MaterialKind::Unknown13 => [185, 224],
        MaterialKind::Unphysical => [225, 240],
        MaterialKind::None => [241, 255],
    }
}

#[derive(Debug)]
pub enum PaletteMapping<'a> {
    Original(&'a [Material; 256]),
    Remapped(Box<([Material; 256], [u8; 256])>),
}

impl PaletteMapping<'_> {
    fn materials_as_ref(&self) -> &[Material; 256] {
        match self {
            PaletteMapping::Original(original) => original,
            PaletteMapping::Remapped(remapped) => &remapped.0,
        }
    }
}

/// Rearrange materials of a palette so that the materials are at the correct
/// indices
#[allow(clippy::cast_possible_truncation)] // Never
fn remap_materials(orig_palette: &[Material; 256]) -> PaletteMapping {
    for (i, orig_material) in orig_palette.iter().enumerate() {
        if orig_material.kind != MaterialKind::None
            && material_kind_for_index(i as u8) != orig_material.kind
        {
            let mut kind_to_orig: HashMap<MaterialKind, Vec<(usize, Material)>> = HashMap::new();
            let mut indices_orig_to_new: [u8; 256] =
                (0..=255_u8).collect::<Vec<_>>().try_into().unwrap();
            for (i, orig_material) in orig_palette.iter().enumerate() {
                kind_to_orig
                    .entry(orig_material.kind)
                    .or_default()
                    .push((i, orig_material.clone()));
            }
            let mut filler_originals = kind_to_orig.remove(&MaterialKind::None).unwrap_or_default();
            let mut new_palette_interm: [Option<Material>; 256] =
                vec![None; 256].try_into().unwrap();
            let mut forced_filler_orig_indices = Vec::new();
            for (kind, mut kind_originals) in kind_to_orig {
                kind_originals.sort_by_key(|(_, m)| m.replacable);
                let range = range_for_material_kind(kind);
                let mut kind_originals_iter = kind_originals.into_iter();
                for (new_i, (orig_i, material)) in
                    (range[0]..=range[1]).zip(kind_originals_iter.by_ref())
                {
                    indices_orig_to_new[orig_i] = new_i;
                    new_palette_interm[new_i as usize] = Some(material);
                }
                filler_originals.extend(kind_originals_iter.inspect(|(orig_i, material)| {
                    if !material.replacable {
                        forced_filler_orig_indices.push(*orig_i);
                    }
                }));
            }
            for (new_i, ok) in new_palette_interm.iter_mut().enumerate() {
                if ok.is_none() {
                    let (orig_i, none_material) = if matches!(new_i, 0 | 255) {
                        filler_originals.remove(0)
                    } else {
                        filler_originals.pop().unwrap()
                    };
                    indices_orig_to_new[orig_i] = new_i as u8;
                    *ok = Some(none_material);
                }
            }
            let new_palette = new_palette_interm.map(Option::unwrap);
            if !forced_filler_orig_indices.is_empty() {
                warn_wrong_indices(
                    forced_filler_orig_indices,
                    orig_palette,
                    &new_palette,
                    &indices_orig_to_new,
                )
            }
            return PaletteMapping::Remapped(Box::new((new_palette, indices_orig_to_new)));
        }
    }
    PaletteMapping::Original(orig_palette)
}

#[cold]
fn warn_wrong_indices(
    forced_filler_orig_indices: Vec<usize>,
    orig_palette: &[Material; 256],
    new_palette: &[Material; 256],
    indices_orig_to_new: &[u8; 256],
) {
    println!(
        "Failed mappings in palette {}: {}",
        hash_n_to_str(compute_hash_n(&new_palette)),
        forced_filler_orig_indices
            .into_iter()
            .map(|orig_i| format!(
                "{:?} -> {:?} ({})",
                orig_palette[orig_i].kind,
                material_kind_for_index(indices_orig_to_new[orig_i]),
                indices_orig_to_new[orig_i],
            ))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

impl SceneWriter<'_> {
    pub fn write_scene(&self) -> Result<(), Box<dyn Error>> {
        let mod_dir = &self.mod_dir;
        let level_dir = mod_dir.join(&self.name);
        fs::create_dir_all(&self.vox_store.lock().unwrap().hash_vox_dir)?;
        fs::create_dir_all(mod_dir)?;
        if let Err(err) = fs::create_dir(&level_dir) {
            if err.kind() != ErrorKind::AlreadyExists {
                return Err(err.into());
            }
        }
        #[rustfmt::skip]
        let palette_mappings = self.scene.palettes.iter()
            .map(|palette| remap_materials(&palette.materials))
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        let palette_files = {
            let mut vox_store = self.vox_store.lock().unwrap();
            vox_store.load_palettes(
                palette_mappings.iter()
                    .map(PaletteMapping::materials_as_ref)
                    .collect::<Vec<_>>()
                    .as_ref(),)
        };
        let mut palette_voxel_data: Vec<Vec<Voxels>> = iter::repeat(Vec::new())
            .take(self.scene.palettes.len())
            .collect();
        let mut entity_voxels: HashMap<u32, Voxels> = HashMap::new();
        for entity in self.scene.iter_entities() {
            if let EntityKind::Shape(shape) = &entity.kind {
                let mut voxels: Voxels = shape.voxels.clone();
                if let Some(PaletteMapping::Remapped(remapped)) =
                    palette_mappings.get(shape.palette as usize)
                {
                    let indices_orig_to_new = remapped.1;
                    let mut palette_index_runs = voxels.palette_index_runs.clone().into_owned();
                    for [_n_times, palette_index] in palette_index_runs.array_chunks_mut() {
                        if *palette_index != 0 {
                            *palette_index = indices_orig_to_new[*palette_index as usize];
                        }
                    }
                    voxels.palette_index_runs = Cow::Owned(palette_index_runs);
                }
                palette_voxel_data
                    .get_mut(shape.palette as usize)
                    .expect("non-existent palette")
                    .push(voxels.clone());
                entity_voxels.insert(entity.handle, voxels);
            }
        }
        #[rustfmt::skip]
        palette_files.into_iter()
            .zip(palette_voxel_data)
            .par_bridge()
            .for_each(|(palette_file, voxel_data)| {
                voxel_data.par_iter().for_each_with(
                    palette_file,
                    |palette_file, shape_voxel_data| {
                        palette_file.lock().unwrap()
                            .store_voxel_data(&shape_voxel_data)
                    },
                )
            });
        let mut xml_file = File::create(mod_dir.join(format!("{}.xml", &self.name)))?;
        let mut xml_writer = Writer::new(&mut xml_file);
        #[rustfmt::skip]
        let start = BytesStart::owned_name("scene").with_attributes(
            vec![
                ("version", "0.6.2"),
                ("shadowVolume", &join_as_strings(self.scene.shadow_volume.iter())),
            ].into_iter());
        let end = start.to_end();
        xml_writer.write_event(Event::Start(start.clone()))?;
        #[rustfmt::skip]
        xml_writer.write_event(Event::Empty(
            BytesStart::owned_name("spawnpoint").with_attributes(
                flatten_attrs(vec![
                    self.scene.spawnpoint.to_xml_attrs(),
                    vec![("name", "spawnpoint".to_string())]
                ]).iter().map(|(k, v)| (*k, v.as_ref())),),))?;
        #[rustfmt::skip]
        xml_writer.write_event(Event::Empty(
            BytesStart::owned_name("spawnpoint").with_attributes(
                flatten_attrs(vec![
                    self.scene.player.transform.to_xml_attrs(),
                    vec![("name", "player".to_string())]
                ]).iter().map(|(k, v)| (*k, v.as_ref())),),))?;
        self.scene.environment.write_xml(&mut xml_writer)?;
        Self::write_boundary(&self.scene.boundary_vertices, &mut xml_writer)?;
        let entities = self.scene.entities.iter().collect::<Vec<_>>();
        for entity in entities {
            #[rustfmt::skip]
            write_entity_xml(
                entity, &mut xml_writer, self.scene, None,
                false, &entity_voxels, &palette_mappings,)?;
        }
        xml_writer.write_event(Event::End(end))?;
        Ok(())
    }

    fn write_boundary(
        boundary: &[BoundaryVertex],
        writer: &mut Writer<&mut File>,
    ) -> XMLResult<()> {
        let start = BytesStart::owned_name("boundary");
        let start_for_end = start.to_owned();
        writer.write_event(Event::Start(start))?;
        boundary.write_xml(writer)?;
        writer.write_event(Event::End(start_for_end.to_end()))?;
        Ok(())
    }
}

fn convert_material(material: &Material) -> VoxMaterial {
    let Rgba([r, g, b, a]) = material.rgba;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let mut vox_mat =
        VoxMaterial::new_color([r, g, b, a].map(|comp| (comp * 255.).clamp(0., 255.) as u8));
    vox_mat.ior = Some(0.3);
    vox_mat.spec_p = Some(0.);
    vox_mat.weight = Some(if material.shinyness > 0.0 {
        material.reflectivity
    } else {
        0.0
    });
    vox_mat.rough = Some(1.0 - material.metalness);
    vox_mat.kind = if vox_mat.rgba[3] < 255 {
        vox_mat.alpha = Some(0.5);
        VoxMaterialKind::Glass
    } else if material.emission > 0.0 {
        VoxMaterialKind::Emit
    } else if material.metalness == 0.0 && material.reflectivity == 0.0 {
        VoxMaterialKind::Diffuse
    } else {
        // To match the original files. Is interpreted as metal if together with spec_p
        // and weight.
        if material.shinyness > 0.0 {
            VoxMaterialKind::Metal
        } else {
            VoxMaterialKind::Plastic
        }
    };
    vox_mat
}

fn join_as_strings<I: IntoIterator<Item = U>, U: ToString>(iter: I) -> String {
    let mut item_strings = iter.into_iter().map(|element| element.to_string());
    let mut joined = if let Some(first) = item_strings.next() {
        first
    } else {
        return String::new();
    };
    for item_string in item_strings {
        joined += " ";
        joined += &item_string;
    }
    joined
}

fn vox_corrected_transform(parent: Option<&Entity>) -> Option<Transform> {
    parent.and_then(|parent| {
        parent.transform().map(|transform: &Transform| {
            if let EntityKind::Shape(shape) = &parent.kind {
                transform_shape(&transform, shape.voxels.size)
            } else {
                transform.clone()
            }
        })
    })
}

impl ToXMLAttributes for Vehicle<'_> {
    fn to_xml_attrs(&self) -> Vec<(&'static str, String)> {
        let props = &self.properties;
        vec![
            ("driven", "false".into()),
            (
                "sound",
                format!("{} {}", props.sound.name, props.sound.pitch),
            ),
            ("spring", props.spring.to_string()),
            ("damping", props.damping.to_string()),
            //("topspeed", ),
            ("acceleration", props.acceleration.to_string()),
            ("strength", props.strength.to_string()),
            ("antispin", props.antispin.to_string()),
            ("antiroll", props.antiroll.to_string()),
            ("difflock", self.difflock.to_string()),
            ("steerassist", props.steerassist.to_string()),
            ("friction", props.friction.to_string()),
        ]
    }
}

impl ToXMLAttributes for Water {
    fn to_xml_attrs(&self) -> Vec<(&'static str, String)> {
        vec![
            ("type", "polygon".to_string()),
            ("depth", self.depth.to_string()),
            ("wave", self.wave.to_string()),
            ("ripple", self.ripple.to_string()),
            ("motion", self.motion.to_string()),
            ("foam", self.foam.to_string()),
        ]
    }
}

impl ToXMLAttributes for Script<'_> {
    #[rustfmt::skip]
    fn to_xml_attrs(&self) -> Vec<(&'static str, String)> {
        iter::once(("file",
            Path::new(self.path).strip_prefix("data/script/")
            .map_or_else(|_| self.path.into(), |ok| ok.display().to_string())))
        .chain(
            ["param0", "param1", "param2", "param3"].iter().copied()
            .zip(self.params.0.iter().map(|(key, value)| format!("{}={}", key, value))))
        .collect()
    }
}

impl ToXMLAttributes for Body {
    fn to_xml_attrs(&self) -> Vec<(&'static str, String)> {
        vec![("dynamic", (self.dynamic == 1).to_string())]
    }
}

fn joint_xml(joint: &Joint) -> (&'static str, Vec<(&'static str, String)>) {
    if joint.kind == JointKind::Rope {
        ("rope", vec![])
    } else {
        ("joint", vec![(
            "type",
            match joint.kind {
                JointKind::Ball => "ball",
                JointKind::Hinge => "hinge",
                JointKind::Prismatic => "prismatic",
                JointKind::Rope => unreachable!(),
            }
            .to_string(),
        )])
    }
}

#[allow(dead_code)]
fn debug_write_entity_positions(entity: &Entity, parent: Option<&Entity>) {
    println!(
        "{:>8} {:<8}: {:+05.1?} {:+05.1?} {:+05.1?}", //  {:+05.1?}
        format!("{:?}", EntityKindVariants::from(&entity.kind)),
        entity.tags.0.iter().next().map_or("", |tag| tag.0),
        entity.transform().map(ToOwned::to_owned).map(|mut x| {
            x.pos = x.pos.map(|dim| dim * 10.);
            x
        }),
        entity.kind.z_u8_start(),
        // {
        //     let mut trans = parent_transform.clone();
        //     trans.pos = trans.pos.map(|dim| dim * 10.);
        //     trans
        // }
        parent.and_then(
            |parent| parent.transform().map(ToOwned::to_owned).map(|mut x| {
                x.pos = x.pos.map(|dim| dim * 10.);
                x
            })
        )
    );
}

pub fn write_entity_xml<W: Write>(
    entity: &Entity,
    writer: &mut Writer<W>,
    scene: &Scene,
    parent: Option<&Entity>,
    mut dynamic: bool,
    entity_voxels: &HashMap<u32, Voxels>,
    palette_remappings: &[PaletteMapping],
) -> XMLResult<()> {
    // debug_write_entity_positions(entity, parent);
    let (name, mut kind_attrs) = match &entity.kind {
        EntityKind::Body(body) => {
            #[rustfmt::skip]
            // Skip the body in wheels, and write the shape inside directly
            if matches!(parent, Some(Entity { kind: EntityKind::Wheel(_), .. })) {
                assert_eq!(entity.children.len(), 1);
                return write_entity_xml(&entity.children[0], writer, scene, Some(entity), dynamic, entity_voxels, palette_remappings)
            }
            dynamic = body.dynamic == 1;
            ("body", body.to_xml_attrs())
        }
        #[rustfmt::skip]
        EntityKind::Shape(shape) => ("vox", vec![
            ("file",
                format!("hash/{}.vox",
                    hash_n_to_str(compute_hash_n(
                        palette_remappings[shape.palette as usize].materials_as_ref()
                    ))),),
            ("object",
                hash_n_to_str(compute_hash_n(entity_voxels.get(&entity.handle).unwrap()))),
            ("texture",
                format!("{} {}", shape.texture_tile, shape.texture_weight)),
            ("density", shape.density.to_string()),
            ("strength", shape.strength.to_string()),
        ]),
        EntityKind::Script(script) => ("script", script.to_xml_attrs()),
        EntityKind::Vehicle(vehicle) => ("vehicle", vehicle.to_xml_attrs()),
        EntityKind::Wheel(_) => ("wheel", vec![]),
        EntityKind::Joint(joint) => joint_xml(joint),
        EntityKind::Light(light) => ("light", light.to_xml_attrs()),
        EntityKind::Location(_) => ("location", vec![]),
        EntityKind::Screen(_) => ("screen", vec![]),
        EntityKind::Trigger(_) => ("trigger", vec![]),
        EntityKind::Water(water) => ("water", water.to_xml_attrs()),
    };
    let start = BytesStart::owned_name(name);
    let mut attrs = Vec::new();
    if let Some(mut world_transform) = vox_corrected_transform(Some(entity)) {
        // If parent body is dynamic, then light is relative to shape in the save
        // representation
        if let Some(parent_transform) = vox_corrected_transform(parent) {
            #[rustfmt::skip]
            let parent_is_vehicle = matches!(parent, Some(Entity { kind: EntityKind::Vehicle(_), .. }));
            world_transform = if dynamic && !parent_is_vehicle {
                world_transform
            } else {
                let mut world_transform_isometry: Isometry3<f32> = world_transform.into();
                let parent_isometry: Isometry3<f32> = parent_transform.into();
                world_transform_isometry = parent_isometry.inv_mul(&world_transform_isometry);
                world_transform_isometry.into()
            };
        }
        attrs.append(&mut world_transform.to_xml_attrs());
    }
    if !entity.tags.0.is_empty() {
        #[rustfmt::skip]
        attrs.push(("tags",
            join_as_strings(entity.tags.0.iter()
                .map(|(&k, &v)| {
                    if v.is_empty() { k.into() } else { format!("{}={}", k, v) }}))));
    }
    if !entity.desc.is_empty() {
        attrs.push(("desc", entity.desc.to_owned()));
    }
    attrs.push(("name", entity.handle.to_string()));
    attrs.append(&mut kind_attrs);
    let start = start.with_attributes(attrs.iter().map(|(k, v)| (*k, v.as_ref())));
    let end = start.to_end().into_owned();
    writer.write_event(Event::Start(start))?;
    for child in &entity.children {
        #[rustfmt::skip]
        write_entity_xml(child, writer, scene, Some(entity), dynamic, entity_voxels, palette_remappings)?;
    }
    match &entity.kind {
        EntityKind::Water(water) => {
            water.boundary_vertices.as_slice().write_xml(writer)?;
        }
        #[rustfmt::skip]
        EntityKind::Joint(Joint { rope: Some(Rope { knots, .. }), .. }) => {
            for pos in [knots.first().map(|knot| knot.from), knots.last().map(|knot| knot.to)].iter().flatten() {
                writer.write_event(Event::Empty(
                    BytesStart::owned_name("location")
                        .with_attributes(vec![("pos", join_as_strings(pos.iter()).as_ref())]),
                ))?;
            }
        },
        _ => {}
    }
    writer.write_event(Event::End(end))?;
    Ok(())
}
fn transform_shape(transform: &Transform, size_i: [u32; 3]) -> Transform {
    let (mut pos, mut rot) = transform.as_nalegbra_pair();
    // println!("# from_raw # pos: {:?}, rot: {:?}, size: {:?}", pos, rot, size_i);
    pos.iter_mut().for_each(|dim| *dim *= 10.0);
    // println!("# from # pos: {:?}, rot: {:?}, size: {:?}", pos, rot, size_i);

    let size = Vector3::from_iterator(size_i.iter().map(|&dim| (dim - dim % 2) as f32));
    let axis_relative_offset = Vector3::new(0.5, 0.5, 0.0);
    let axis_offset = size.component_mul(&axis_relative_offset);
    pos += rot.transform_vector(&axis_offset);
    rot *= UnitQuaternion::from_axis_angle(&Vector3::x_axis(), TAU / 4.);

    // println!("# into # pos: {:?}, rot: {:?}, size: {:?}", pos, rot, size_i);
    pos.iter_mut().for_each(|dim| *dim *= 0.1);
    (pos, rot).into()
}

#[derive(Debug)]
enum MaterialGroup {
    None,
    Glass,
    Grass,
    Dirt,
    Plastic,
    Wood,
    Plaster,
    Concrete,
    Brick,
    WeakMetal,
    HardMasonry,
    HardMetal,
    HeavyMetal,
    Rock,
    Unphysical,
    Reserved(u8),
}

impl From<PaletteIndex> for MaterialGroup {
    fn from(palette_index: PaletteIndex) -> Self {
        #[allow(clippy::enum_glob_use)]
        use MaterialGroup::*;
        if palette_index.0 == 0 {
            return None;
        }
        match (palette_index.0 - 1) / 8 {
            0 => Glass,
            1 | 2 => Grass,
            3 | 4 => Dirt,
            5 | 6 => Rock,
            7 | 8 => Wood,
            9 | 10 => Concrete,
            11 | 12 => Brick,
            13 | 14 => Plaster,
            15 | 16 => WeakMetal,
            17 | 18 => HeavyMetal,
            19 | 20 => Plastic,
            21 => HardMetal,
            22 => HardMasonry,
            28 | 29 => Unphysical,
            n => Reserved(n),
        }
    }
}

const BASE64_CONFIG: base64::Config = base64::Config::new(base64::CharacterSet::UrlSafe, false);

pub fn compute_hash_n<H: Hash>(to_hash: &H) -> u64 {
    let mut hasher = DefaultHasher::new();
    to_hash.hash(&mut hasher);
    hasher.finish()
}

#[must_use]
pub fn hash_n_to_str(n: u64) -> String {
    base64::encode_config(n.to_le_bytes(), BASE64_CONFIG)
}

fn hash_str_to_n(string: &str) -> Result<u64, ()> {
    match base64::decode_config(string, BASE64_CONFIG) {
        Ok(bytes) => Ok(u64::from_le_bytes(bytes.try_into().map_err(|_| ())?)),
        Err(_) => Err(()),
    }
}

#[allow(clippy::approx_constant, clippy::unreadable_literal)]
#[cfg(test)]
pub mod transform_shape_tests {
    use approx::assert_relative_eq;

    use super::*;

    fn rot(x: f32, y: f32, z: f32) -> [f32; 4] {
        let quat =
            UnitQuaternion::from_euler_angles(x.to_radians(), y.to_radians(), z.to_radians());
        [quat.i, quat.j, quat.k, quat.w]
    }

    #[test]
    fn at_origin_no_rotation() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [-0.5, 0.0, 0.5],
                    rot: [-0.7071068, 0.0, 0.0, 0.7071068]
                },
                [10, 10, 10]
            ),
            Transform {
                pos: [0., 0., 0.],
                rot: rot(0., 0., 0.)
            }
        )
    }

    #[test]
    fn at_origin_45_x() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [-0.5, -0.3535534, 0.35355335],
                    rot: [-0.38268343, 0.0, 0.0, 0.92387956]
                },
                [10, 10, 10]
            ),
            Transform {
                pos: [0., 0., 0.],
                rot: rot(45., 0., 0.)
            }
        )
    }

    #[test]
    fn at_origin_45_y() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [0.000000059604645, 0.0, 0.70710677],
                    rot: [-0.6532815, 0.27059808, 0.27059808, 0.6532815]
                },
                [10, 10, 10]
            ),
            Transform {
                pos: [0., 0., 0.],
                rot: rot(0., 45., 0.)
            }
        )
    }

    #[test]
    fn at_origin_90_y() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [0.5, 0.0, 0.49999994],
                    rot: [-0.5, 0.5, 0.5, 0.5]
                },
                [10, 10, 10]
            ),
            Transform {
                pos: [0., 0., 0.],
                rot: rot(0., 90., 0.)
            }
        )
    }

    #[test]
    fn at_origin_20_z() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [-0.4698462, -0.1710101, 0.4999998],
                    rot: [-0.6963643, -0.12278781, 0.12278781, 0.6963643]
                },
                [10, 10, 10]
            ),
            Transform {
                pos: [0., 0., 0.],
                rot: rot(0., 0., 20.)
            }
        )
    }

    #[test]
    fn at_origin_45_45_45() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [0.17677675, -0.60355335, 0.32322317],
                    rot: [-0.19134167, 0.19134174, 0.46193975, 0.8446232]
                },
                [10, 10, 10]
            ),
            Transform {
                pos: [0., 0., 0.],
                rot: rot(45., 45., 45.)
            }
        )
    }

    #[test]
    fn positive_x() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [1.5, 0.0, 0.5],
                    rot: [-0.7071068, 0.0, 0.0, 0.7071068]
                },
                [10, 10, 10]
            ),
            Transform {
                pos: [2.0, 0.0, 0.0],
                rot: rot(0., 0., 0.)
            }
        )
    }

    #[test]
    fn negative_x() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [-2.5, 0.0, 0.5],
                    rot: [-0.7071068, 0.0, 0.0, 0.7071068]
                },
                [10, 10, 10]
            ),
            Transform {
                pos: [-2.0, 0.0, 0.0],
                rot: rot(0., 0., 0.)
            }
        )
    }

    #[test]
    fn odd_z() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [-0.5, 0.0, 1.0],
                    rot: [-0.7071068, 0.0, 0.0, 0.7071068]
                },
                [10, 1, 1]
            ),
            Transform {
                pos: [0.0, 0.0, 1.0],
                rot: rot(0., 0., 0.)
            }
        )
    }

    #[test]
    fn odd_negative_z() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [-0.5, 0.0, -1.0],
                    rot: [-0.7071068, 0.0, 0.0, 0.7071068]
                },
                [10, 1, 1]
            ),
            Transform {
                pos: [0.0, 0.0, -1.0],
                rot: rot(0., 0., 0.)
            }
        )
    }

    #[test]
    fn odd_at_origin() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [-0.4, 0.0, 0.1],
                    rot: [-0.7071068, 0.0, 0.0, 0.7071068]
                },
                [9, 3, 7]
            ),
            Transform {
                pos: [0.0, 0.0, 0.0],
                rot: rot(0., 0., 0.)
            }
        )
    }

    #[test]
    fn origin_xy_45() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [-0.10355337, -0.3535534, 0.6035534],
                    rot: [-0.35355338, 0.35355344, 0.1464466, 0.8535534]
                },
                [10, 10, 10]
            ),
            Transform {
                pos: [0.0, 0.0, 0.0],
                rot: rot(45., 45., 0.)
            }
        )
    }
}
