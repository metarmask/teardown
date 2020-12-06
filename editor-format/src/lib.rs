#![feature(array_map)]
use std::{collections::HashSet, convert::TryInto, error::Error, f32::consts::TAU, fs::{self, File}, io::{Write, BufWriter, ErrorKind}, path::Path};
use nalgebra::{Isometry3, UnitQuaternion, Vector3};
pub(crate) use quick_xml::Result as XMLResult;
use quick_xml::{Writer, events::{BytesStart, Event}};
use teardown_bin_format::{Entity, EntityKind, EntityKindVariants, Environment, Light, MaterialKind, PaletteIndex, Rgba, Scene, Transform, Vehicle, VehicleProperties, VoxelData, compute_hash_str};
use vox::semantic::{Material as VoxMaterial, MaterialKind as VoxMaterialKind, Model, Node, VoxFile, Voxel};

pub fn voxel_data_to_vox_node(voxel_data: &VoxelData<'_>) -> Node {
    let mut voxels = Vec::new();
    if voxel_data.size.iter().all(|&dim| dim < 256) {
        for ([x, y, z], palette_index) in voxel_data.iter() {
            let coord = [x, y, z];
            voxels.push(Voxel {
                pos: coord.iter().copied().map(TryInto::try_into).collect::<Result<Vec<_>, _>>().unwrap().try_into().unwrap(),
                index: palette_index,
            });
        }
    }
    let [x, y, z] = voxel_data.size.map(|x| (x as i32) / 2);
    // 
    let node = Node::new([x, y-1, z], Model::new(voxel_data.size, voxels));
    node
}

trait WriteXML {
    fn write_xml<W: Write>(&self, writer: &mut Writer<W>) -> XMLResult<()>;
}

trait ToXMLAttributes {
    fn to_xml_attrs(&self) -> Vec<(&'static str, String)>;
}

impl<'a> WriteXML for Environment<'a> {
    fn write_xml<W: Write>(&self, writer: &mut Writer<W>) -> XMLResult<()> {
        let start = BytesStart::owned_name("environment")
            .with_attributes(vec![
                ("skybox", self.skybox.texture),
                ("skyboxtint", &join_as_strings(self.skybox.tint.0.iter())),
                ("skyboxbright", self.skybox.brightness.to_string().as_ref()),
                ("skyboxrot", self.skybox.rotation.to_string().as_ref()),
                ("ambient", self.ambient_from_skybox.to_string().as_ref()),
                ("fogColor", &join_as_strings(self.fog.color.0.iter())),
                ("fogParams", &join_as_strings([self.fog.start, self.fog.end, self.fog.amount, self.fog.exponent].iter())),
                ("sunBrightness", self.sun.brightness.to_string().as_ref())
            ].into_iter());
            
        writer.write_event(Event::Empty(start.clone()))?;
        
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

pub fn write_scene<P: AsRef<Path>>(scene: &Scene, mod_dir: P, name: &str) -> Result<(), Box<dyn Error>> {
    let mod_dir = mod_dir.as_ref();
    let level_dir = mod_dir.join(name);
    if let Err(err) = fs::create_dir(&level_dir) {
        if err.kind() != ErrorKind::AlreadyExists { return Err(err.into()) }
    }
    let mut xml_file = File::create(mod_dir.join(format!("{}.xml", name)))?;
    let mut xml_writer = Writer::new(&mut xml_file);
    let start = BytesStart::owned_name("scene")
        .with_attributes(vec![
            ("version", "0.5.5"),
            ("shadowVolume", &join_as_strings(scene.shadow_volume.iter()))
        ].into_iter());
    let end = start.to_end();
    xml_writer.write_event(Event::Start(start.clone()))?;
    scene.environment.write_xml(&mut xml_writer)?;
    for child in scene.entities.iter() {
        write_entity_xml(child, &mut xml_writer, scene, &Transform::default())?;
    }
    xml_writer.write_event(Event::End(end))?;
    for (i, palette) in scene.palettes.iter().enumerate() {
        let vox = palette_to_vox(scene, i);
        let file_name = format!("{}.vox", compute_hash_str(&palette.materials));
        let mut vox_writer = BufWriter::new(File::create(level_dir.join(file_name))?);
        vox.write(&mut vox_writer)?;
    }
    Ok(())
}

fn palette_to_vox(scene: &Scene, palette_index: usize) -> VoxFile {
    let mut file = VoxFile::new();
    let td_materials = &scene.palettes[palette_index as usize].materials;
    let mut vox_materials = Vec::new();
    for material in td_materials.iter().skip(1) {
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
            // vox_mat.att = Some(0.);
            // vox_mat.g1 = Some(-0.5);
            // vox_mat.ldr = Some(0.);
            // vox_mat.flux = Some(0.);
            // vox_mat.spec = Some(1.);
            // vox_mat.g0 = Some(-0.5);
            // vox_mat.gw = Some(0.7);
            // vox_mat.metal = Some(0.0); // !?
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
        // vox_mat.spec = Some(material.shinyness);
        // vox_mat.att = Some(material.reflectivity);
        vox_materials.push(vox_mat);
    }
    file.set_palette(&vox_materials).unwrap();
    // let syntaxical = vox::syntax::VoxFile::try_from(file.clone()).unwrap();
    // for chunk in syntaxical.main_chunk.children.iter() {
    //     match &chunk.kind {
    //         ChunkKind::Material(material) => {
    //             println!("{:?}", material);
    //         },
    //         _ => {}
    //     }
    // }
    let mut voxel_data_set = HashSet::new();
    for entity in scene.iter_entities() {
        if let EntityKind::Shape(shape) = &entity.kind {
            if shape.palette as usize == palette_index {
                voxel_data_set.insert(&shape.voxel_data);
                if entity.desc == "printthis" {
                    println!("{:#?}", td_materials.iter().filter(|mat| mat.kind != MaterialKind::None).collect::<Vec<_>>());
                }
            }
        }
    }
    for voxel_data in voxel_data_set {
        let mut node = voxel_data_to_vox_node(voxel_data);
        node.name = Some(compute_hash_str(voxel_data));
        file.root.add(node);
    }
    file
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
    println!("{:>8} {:<8}: {:+05.1?} {:+05.1?}",
        format!("{:?}", EntityKindVariants::from(&entity.kind)),
        if let Some(tag) = entity.tags.0.iter().next() { tag.0 } else { "" },
        entity.transform().map(ToOwned::to_owned).map(|mut x| {
            x.pos = x.pos.map(|dim| dim * 10.);
            x
        }),
        {
            let mut trans = parent_transform.clone();
            trans.pos = trans.pos.map(|dim| dim * 10.);
            trans
        }
    );
    let (name, mut kind_attrs) = match &entity.kind {
        EntityKind::Body(_) => {
            ("body", vec![])
        }
        EntityKind::Shape(shape) => {
            let mut kind_attrs = vec![
                ("texture", format!("{} {}", shape.texture_tile, shape.texture_weight))
            ];
            let xml_tag = if shape.voxel_data.size.iter().any(|&dim| dim > 256) {
                kind_attrs.push(("size", join_as_strings(shape.voxel_data.size.iter())));
                is_voxbox = true;
                "voxbox"
            } else {
                kind_attrs.append(&mut vec![
                    ("file", format!("LEVEL/{}.vox", compute_hash_str(&scene.palettes[shape.palette as usize].materials))),
                    ("object", compute_hash_str(&shape.voxel_data)),
                    //("size", join_as_strings(shape.size.iter().map(|dim| *dim as f32)))
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
            ("wheel", vec![])
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
        // let mut world_isometry: Isometry3<f32> = world_transform.into();
        // let mut inverse_isometry: Isometry3<f32> = inverse_transform.into();

        // // Actual calculation
        // world_isometry *= inverse_isometry.inverse();
        // inverse_isometry *= world_isometry;

        // inverse_transform = inverse_isometry.into();
        // world_transform = world_isometry.into();
        let isometry: Isometry3<f32> = world_transform.into();
        let parent_isometry: Isometry3<f32> = parent_transform.to_owned().into();
        // isometry *= parent_isometry.inverse();
        isometry.inv_mul(&parent_isometry);
        println!("{:?}", parent_isometry);
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
    // .transform_vector(&aligned_relative_offset)
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
