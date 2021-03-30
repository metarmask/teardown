#![feature(array_map)]
use std::{collections::{HashMap, hash_map::{DefaultHasher, Entry}}, convert::{TryInto, TryFrom}, error::Error, f32::consts::TAU, fs::{self, File}, hash::{Hash, Hasher}, io::{ErrorKind, Write}, iter, path::{Path, PathBuf}, sync::{Arc, Mutex}};
use nalgebra::{Isometry3, UnitQuaternion, Vector3};
pub(crate) use quick_xml::Result as XMLResult;
use quick_xml::{Writer, events::{BytesStart, Event}};
use teardown_bin_format::{Entity, EntityKind, Environment, Exposure, Light, Material, Palette, PaletteIndex, Rgba, Scene, Sound, Transform, Vehicle, VehicleProperties, VoxelData, environment::{self, Fog, Skybox, Sun}};
use vox::semantic::{Material as VoxMaterial, MaterialKind as VoxMaterialKind, Model, Node, VoxFile, Voxel};
use derive_builder::Builder;
use rayon::prelude::*;

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
            ("fogParams", join_as_strings([self.start, self.start + self.distance, self.amount, self.exponent].iter()))
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
                ("skybox", PathBuf::from(self.texture).strip_prefix("data/env").map(|x| x.display().to_string()).unwrap_or(self.texture.to_string())),
                ("skyboxtint", join_as_strings(self.tint.0.iter())),
                ("skyboxbright", self.brightness.to_string()),
                ("skyboxrot", self.rotation.to_string()),
                ("ambient", self.ambient_light.to_string()),
            ],
            self.sun.to_xml_attrs()
        ])
    }
}

impl ToXMLAttributes for Sun {
    fn to_xml_attrs(&self) -> Vec<(&'static str, String)> {
        vec![
            ("sunBrightness", self.brightness.to_string()),
            ("sunColorTint", join_as_strings(self.tint.0.iter())),
            ("sunDir", join_as_strings(self.direction.iter())),
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
        vec![(self.0, join_as_strings([self.1.path, self.1.volume.to_string().as_ref()].iter()))]
    }
}

impl<'a> WriteXML for Environment<'a> {
    fn write_xml<W: Write>(&self, writer: &mut Writer<W>) -> XMLResult<()> {
        writer.write_event(Event::Empty(
            BytesStart::borrowed_name("environment".as_bytes())
            .with_attributes(flatten_attrs(vec![
                self.skybox.to_xml_attrs(),
                self.exposure.to_xml_attrs(),
                self.fog.to_xml_attrs(),
                self.water.to_xml_attrs(),
                vec![
                    ("nightlight", self.nightlight.to_string()),
                    ("ambience", join_as_strings([self.ambience.path, self.ambience.volume.to_string().as_ref()].iter())),
                    ("slippery", self.slippery.to_string())],
                self.fog.to_xml_attrs()
            ]).iter().map(|(k, v)| (*k, v.as_ref())))
        ))?;
        Ok(())
    }
}

impl ToXMLAttributes for Transform {
    fn to_xml_attrs(&self) -> Vec<(&'static str, String)> {
        let mut attrs = Vec::new();
        let (pos, rot) = self.into_nalegbra_pair();
        attrs.push(("pos", join_as_strings(pos.iter())));
        attrs.push(("rot", join_as_strings({
            let (x, y, z) = rot.euler_angles();
            [x, y, z].map(|dim| dim.to_degrees()).iter()
        })));
        attrs
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
    dirty: bool
}

impl VoxStoreFile {

    fn new(path: PathBuf, palette: &Palette) -> Result<Self, Box<dyn Error + Send>> {
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
            file.set_palette(&palette.materials.iter().skip(1).map(convert_material).collect::<Vec<_>>()).unwrap();
            file
        };
        Ok(Self { dirty: false, vox, shape_indices, path })
    }

    fn store_voxel_data(&mut self, voxel_data: &VoxelData) {
        let hash_n = compute_hash_n(&voxel_data);
        if !self.shape_indices.contains_key(&hash_n) {
            let len = self.vox.root.children().map(|c| c.len()).unwrap_or_default();
            let mut voxels = Vec::new();
            for (coord, palette_index) in voxel_data.iter() {
                if let Ok(pos) = coord.iter().copied().map(TryInto::try_into).collect::<Result<Vec<_>, _>>() {
                    voxels.push(Voxel {
                        pos: <[u8; 3]>::try_from(pos).unwrap(),
                        index: palette_index,
                    });
                }
            }
            let model = Model::new(voxel_data.size.map(|dim| dim.min(256)), voxels);
            let [x, y, z] = model.size().map(|x| (x as i32) / 2);
            let mut node = Node::new([x, y-1, z], model);
            node.name = Some(hash_n_to_str(hash_n));
            self.vox.root.add(node);
            self.shape_indices.insert(hash_n, len);
            self.dirty = true;
        }
    }

    fn write(&mut self) -> Result<(), Box<dyn Error>> {
        self.vox.to_owned()
            .write(&mut File::create(&self.path).unwrap()).unwrap();
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
            return Err("data/vox didn't exist in teardown_dir".into())
        }
        Ok(Arc::new(Mutex::new(Self {
            hash_vox_dir: vox_dir.join("hash"),
            palette_files: Default::default()
        })))
    }

    fn load_palettes(&mut self, palettes: &[Palette]) -> Vec<Arc<Mutex<VoxStoreFile>>> {
        let mut vec = Vec::new();
        for palette in palettes {
            let hash_n = compute_hash_n(&palette.materials);
            vec.push(match self.palette_files.entry(hash_n) {
                Entry::Occupied(occupant) =>
                    occupant.get().to_owned(),
                Entry::Vacant(vacancy) =>
                    vacancy.insert(Arc::new(Mutex::new(
                        VoxStoreFile::new(
                            self.hash_vox_dir.join(format!("{}.vox", hash_n_to_str(hash_n))),
                            palette).unwrap()))).to_owned()
            })
        }
        vec
    }

    pub fn write_dirty(&mut self) -> Result<(), Box<dyn Error>> {
        for (_, file) in self.palette_files.iter() {
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

impl SceneWriter<'_> {
    pub fn write_scene(&self) -> Result<(), Box<dyn Error>> {
        let mod_dir = &self.mod_dir;
        let level_dir = mod_dir.join(&self.name);
        fs::create_dir_all(&self.vox_store.lock().unwrap().hash_vox_dir)?;
        fs::create_dir_all(mod_dir)?;
        if let Err(err) = fs::create_dir(&level_dir) {
            if err.kind() != ErrorKind::AlreadyExists { return Err(err.into()) }
        }
        let mut xml_file = File::create(mod_dir.join(format!("{}.xml", &self.name)))?;
        let mut xml_writer = Writer::new(&mut xml_file);
        let start = BytesStart::owned_name("scene")
            .with_attributes(vec![
                ("version", "0.5.5"),
                ("shadowVolume", &join_as_strings(self.scene.shadow_volume.iter()))
            ].into_iter());
        let end = start.to_end();
        xml_writer.write_event(Event::Start(start.clone()))?;
        self.scene.environment.write_xml(&mut xml_writer)?;
        let entities = self.scene.entities.iter().collect::<Vec<_>>();
        for entity in entities {
            write_entity_xml(entity, &mut xml_writer, self.scene, &Transform::default())?;
        }
        xml_writer.write_event(Event::End(end))?;
        let palette_files = {
            let mut vox_store = self.vox_store.lock().unwrap();
            vox_store.load_palettes(&self.scene.palettes)
        };
        let mut palette_voxel_data: Vec<Vec<&VoxelData>> = iter::repeat(Vec::new()).take(self.scene.palettes.len()).collect();
        for entity in self.scene.iter_entities() {
            if let EntityKind::Shape(shape) = &entity.kind {
                palette_voxel_data.get_mut(shape.palette as usize).expect("non-existent palette").push(&shape.voxel_data);
            }
        }
        palette_files.into_iter().zip(palette_voxel_data).par_bridge().for_each(|(palette_file, voxel_data)| {
            voxel_data.par_iter().for_each_with(palette_file, |palette_file, shape_voxel_data| {
                palette_file.lock().unwrap().store_voxel_data(&shape_voxel_data)
            })
        });
        Ok(())
    }
}

fn convert_material(material: &Material) -> VoxMaterial {
    let Rgba([r, g, b, a]) = material.rgba;
    let mut vox_mat = VoxMaterial::new_color([r, g, b, a].map(|comp| (comp * 255.).clamp(0., 255.) as u8));
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
    } else {
        if material.metalness == 0.0 && material.reflectivity == 0.0 {
            VoxMaterialKind::Diffuse
        } else {
            // To match the original files. Is interpreted as metal if together with spec_p and weight.
            if material.shinyness > 0.0 {
                VoxMaterialKind::Metal
            } else {
                VoxMaterialKind::Plastic
            }
        }
    };
    vox_mat
}

fn join_as_strings<I: IntoIterator<Item = U>, U: ToString>(iter: I) -> String {
    let mut item_strings = iter.into_iter().map(|element| element.to_string());
    let mut joined = if let Some(first) = item_strings.next() {
        first
    } else {
        return String::new()
    };
    for item_string in item_strings {
        joined += " ";
        joined += &item_string;
    }
    joined
}

pub fn write_entity_xml<W: Write>(entity: &Entity, writer: &mut Writer<W>, scene: &Scene, parent_transform: &Transform) -> XMLResult<()> {
    let mut is_voxbox = false;
    // println!("{:>8} {:<8}: {:+05.1?} {:+05.1?}",
    //     format!("{:?}", EntityKindVariants::from(&entity.kind)),
    //     if let Some(tag) = entity.tags.0.iter().next() { tag.0 } else { "" },
    //     entity.transform().map(ToOwned::to_owned).map(|mut x| {
    //         x.pos = x.pos.map(|dim| dim * 10.);
    //         x
    //     }),
    //     {
    //         let mut trans = parent_transform.clone();
    //         trans.pos = trans.pos.map(|dim| dim * 10.);
    //         trans
    //     }
    // );
    let (name, mut kind_attrs) = match &entity.kind {
        EntityKind::Body(_) => {
            ("body", vec![])
        }
        EntityKind::Shape(shape) => {
            let mut kind_attrs = vec![
                ("texture", format!("{} {}", shape.texture_tile, shape.texture_weight))
            ];
            let xml_tag = if false && shape.voxel_data.size.iter().any(|&dim| dim > 256) {
                kind_attrs.push(("size", join_as_strings(shape.voxel_data.size.iter())));
                is_voxbox = true;
                "voxbox"
            } else {
                kind_attrs.append(&mut vec![
                    ("file", format!("hash/{}.vox", hash_n_to_str(compute_hash_n(&scene.palettes[shape.palette as usize].materials)))),
                    ("object", hash_n_to_str(compute_hash_n(&shape.voxel_data)))
                ]);
                "vox"
            };
            (xml_tag, kind_attrs)
        }
        EntityKind::Script(script) => {

            let kind_attrs = vec![("file", match Path::new(script.path).strip_prefix("data/script/") {
                Ok(ok) => ok.display().to_string(), Err(_) => script.path.to_string()
            })];
            ("script", kind_attrs)
        }
        EntityKind::Vehicle(Vehicle {
            body_handle: _,
            wheel_handles: _,
            properties: VehicleProperties {
                spring,
                damping,
                acceleration,
                strength,
                friction,
                antispin,
                steerassist,
                antiroll,
                sound,
                ..
            },
            player_pos: _,
            difflock,
            refs: _,
            exhausts: _,
            vitals: _,
            arm_rot: _,
            ..
        }) => {
            ("not-vehicle", vec![
                ("driven", "false".into()),
                ("sound", format!("{} {}", sound.name, sound.pitch)),
                ("spring", spring.to_string()),
                ("damping", damping.to_string()),
                //("topspeed", ),
                ("acceleration", acceleration.to_string()),
                ("strength", strength.to_string()),
                ("antispin", antispin.to_string()),
                ("antiroll", antiroll.to_string()),
                ("difflock", difflock.to_string()),
                ("steerassist", steerassist.to_string()),
                ("friction", friction.to_string())
            ])
        }
        EntityKind::Wheel(_) => {
            ("not-wheel", vec![])
        }
        EntityKind::Joint(_) => {
            ("joint", vec![])
        }
        EntityKind::Light(Light {
            ..
        }) => {
            ("light", vec![])
        }
        EntityKind::Location(_) => {
            ("location", vec![])
        }
        EntityKind::Screen(_) => {
            ("screen", vec![])
        }
        EntityKind::Trigger(_) => {
            ("trigger", vec![])
        }
        EntityKind::Water(_) => {
            ("water", vec![])
        }
    };
    let start = BytesStart::owned_name(name);
    let mut attrs = Vec::new();
    if let Some(mut world_transform) = entity.kind.transform().map(ToOwned::to_owned) {
        if let EntityKind::Shape(shape)  = &entity.kind {
            if !is_voxbox {
                world_transform = transform_shape(&world_transform, shape.voxel_data.size)
            }
        }
        if let EntityKind::Light(_) = &entity.kind {
            world_transform = transform_shape(&world_transform, [1, 1, 1]);
        }
        let isometry: Isometry3<f32> = world_transform.into();
        let parent_isometry: Isometry3<f32> = parent_transform.to_owned().into();
        isometry.inv_mul(&parent_isometry);
        let transform: Transform = isometry.into();
        attrs.append(&mut transform.to_xml_attrs());
    }
    if entity.tags.0.len() != 0 {
        attrs.push(("tags", join_as_strings(entity.tags.0.iter().map(|(k, v)| if *v == "" {
            k.to_string()
        } else {
            format!("{}={}", k, v)
        }))));
    }
    if entity.desc != "" {
        attrs.push(("desc", entity.desc.to_owned()));
    }
    attrs.push(("name", entity.handle.to_string()));
    attrs.append(&mut kind_attrs);
    let start = start
        .with_attributes(attrs.iter().map(|(k, v)| (*k, v.as_ref())));
    let end = start.to_end().into_owned();
    writer.write_event(Event::Start(start))?;
    let parent_transform = entity.transform().map(ToOwned::to_owned).unwrap_or_default();
    for child in entity.children.iter() {
        write_entity_xml(child, writer, scene, &parent_transform)?;
    }
    writer.write_event(Event::End(end))?;
    Ok(())
}
fn transform_shape(transform: &Transform, size_i: [u32; 3]) -> Transform {
    let (mut pos, mut rot) = transform.into_nalegbra_pair();
    // println!("# from_raw # pos: {:?}, rot: {:?}, size: {:?}", pos, rot, size_i);
    pos.iter_mut().for_each(|dim| *dim *= 10.0);
    // println!("# from # pos: {:?}, rot: {:?}, size: {:?}", pos, rot, size_i);

    let size = Vector3::from_iterator(size_i.iter().map(|&dim| (dim - dim % 2) as f32));
    let axis_relative_offset = Vector3::new(0.5, 0.5, 0.0);
    let axis_offset = size.component_mul(&axis_relative_offset);
    pos += rot.transform_vector(&axis_offset);
    rot *= UnitQuaternion::from_axis_angle(&Vector3::x_axis(), TAU/4.);

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
    Reserved(u8)
}

impl From<PaletteIndex> for MaterialGroup {
    fn from(palette_index: PaletteIndex) -> Self {
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
            n => Reserved(n)
        }
    }
}

const BASE64_CONFIG: base64::Config = base64::Config::new(base64::CharacterSet::UrlSafe, false);

pub fn compute_hash_n<H: Hash>(to_hash: &H) -> u64 {
    let mut hasher = DefaultHasher::new();
    to_hash.hash(&mut hasher);
    hasher.finish()
}

pub fn hash_n_to_str(n: u64) -> String {
    base64::encode_config(
        n.to_le_bytes(),
        BASE64_CONFIG)
}

fn hash_str_to_n(string: &str) -> Result<u64, ()> {
    match base64::decode_config(string, BASE64_CONFIG) {
        Ok(bytes) => Ok(u64::from_le_bytes(bytes.try_into().map_err(|_| ())?)),
        Err(_) => Err(())
    }
}

#[cfg(test)]
pub mod transform_shape_tests {
    use approx::assert_relative_eq;

    use super::*;

    fn rot(x: f32, y: f32, z: f32) -> [f32; 4] {
        let quat = UnitQuaternion::from_euler_angles(x.to_radians(), y.to_radians(), z.to_radians());
        let w = quat.w;
        let x = quat.i;
        let y = quat.j;
        let z = quat.k;
        [x, y, z, w]
    }

    #[test]
    fn at_origin_no_rotation() {
        assert_relative_eq!(transform_shape(&Transform {
            pos: [-0.5, 0.0, 0.5], rot: [-0.7071068, 0.0, 0.0, 0.7071068] }, [10, 10, 10]),
            Transform {
                pos: [0., 0., 0.],
                rot: rot(0., 0., 0.)
            })
    }

    #[test]
    fn at_origin_45_x() {
        assert_relative_eq!(transform_shape(&Transform {
            pos: [-0.5, -0.3535534, 0.35355335], rot: [-0.38268343, 0.0, 0.0, 0.92387956] }, [10, 10, 10]),
            Transform {
                pos: [0., 0., 0.],
                rot: rot(45., 0., 0.)
            })
    }

    #[test]
    fn at_origin_45_y() {
        assert_relative_eq!(transform_shape(&Transform {
            pos: [0.000000059604645, 0.0, 0.70710677], rot: [-0.6532815, 0.27059808, 0.27059808, 0.6532815] }, [10, 10, 10]),
            Transform {
                pos: [0., 0., 0.],
                rot: rot(0., 45., 0.)
            })
    }

    #[test]
    fn at_origin_90_y() {
        assert_relative_eq!(transform_shape(&Transform {
            pos: [0.5, 0.0, 0.49999994], rot: [-0.5, 0.5, 0.5, 0.5] }, [10, 10, 10]),
            Transform {
                pos: [0., 0., 0.],
                rot: rot(0., 90., 0.)
            })
    }

    #[test]
    fn at_origin_20_z() {
        assert_relative_eq!(transform_shape(&Transform {
            pos: [-0.4698462, -0.1710101, 0.4999998], rot: [-0.6963643, -0.12278781, 0.12278781, 0.6963643] }, [10, 10, 10]),
            Transform {
                pos: [0., 0., 0.],
                rot: rot(0., 0., 20.)
            })
    }

    #[test]
    fn at_origin_45_45_45() {
        assert_relative_eq!(transform_shape(&Transform {
            pos: [0.17677675, -0.60355335, 0.32322317], rot: [-0.19134167, 0.19134174, 0.46193975, 0.8446232] }, [10, 10, 10]),
            Transform {
                pos: [0., 0., 0.],
                rot: rot(45., 45., 45.)
            })
    }

    #[test]
    fn positive_x() {
        assert_relative_eq!(transform_shape(&Transform {
            pos: [1.5, 0.0, 0.5], rot: [-0.7071068, 0.0, 0.0, 0.7071068] }, [10, 10, 10]),
            Transform {
                pos: [2.0, 0.0, 0.0],
                rot: rot(0., 0., 0.)
            })
    }

    #[test]
    fn negative_x() {
        assert_relative_eq!(transform_shape(&Transform {
            pos: [-2.5, 0.0, 0.5], rot: [-0.7071068, 0.0, 0.0, 0.7071068] }, [10, 10, 10]),
            Transform {
                pos: [-2.0, 0.0, 0.0],
                rot: rot(0., 0., 0.)
            })
    }

    #[test]
    fn odd_z() {
        assert_relative_eq!(transform_shape(&Transform {
            pos: [-0.5, 0.0, 1.0], rot: [-0.7071068, 0.0, 0.0, 0.7071068] }, [10, 1, 1]),
            Transform {
                pos: [0.0, 0.0, 1.0],
                rot: rot(0., 0., 0.)
            })
    }

    #[test]
    fn odd_negative_z() {
        assert_relative_eq!(transform_shape(&Transform {
            pos: [-0.5, 0.0, -1.0], rot: [-0.7071068, 0.0, 0.0, 0.7071068] }, [10, 1, 1]),
            Transform {
                pos: [0.0, 0.0, -1.0],
                rot: rot(0., 0., 0.)
            })
    }

    #[test]
    fn odd_at_origin() {
        assert_relative_eq!(transform_shape(&Transform {
            pos: [-0.4, 0.0, 0.1], rot: [-0.7071068, 0.0, 0.0, 0.7071068] }, [9, 3, 7]),
            Transform {
                pos: [0.0, 0.0, 0.0],
                rot: rot(0., 0., 0.)
            })
    }

    #[test]
    fn origin_xy_45() {
        assert_relative_eq!(transform_shape(&Transform {
            pos: [-0.10355337, -0.3535534, 0.6035534], rot: [-0.35355338, 0.35355344, 0.1464466, 0.8535534] }, [10, 10, 10]),
            Transform {
                pos: [0.0, 0.0, 0.0],
                rot: rot(45., 45., 0.)
            })
    }
}
