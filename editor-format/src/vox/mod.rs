use std::{
    borrow::Cow,
    collections::{hash_map::Entry, HashMap},
    convert::TryInto,
    f32::consts::TAU,
    fs::{self, File},
    io::ErrorKind,
    iter,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::bail;
use nalgebra::{UnitQuaternion, Vector3};
use rayon::iter::{IntoParallelRefIterator, ParallelBridge, ParallelIterator};
use teardown_bin_format::{
    EntityKind, Material, MaterialKind, PaletteIndex, Rgba, Transform, Voxels,
};
use vox::semantic::{
    Material as VoxMaterial, MaterialKind as VoxMaterialKind, Model, Node, VoxFile, Voxel,
};

use crate::util::{IntoFixedArray, UnwrapLock};
pub struct Store {
    pub hash_vox_dir: PathBuf,
    pub palette_files: HashMap<u64, Arc<Mutex<StoreFile>>>,
}

pub struct StoreFile {
    path: PathBuf,
    vox: VoxFile,
    shape_indices: HashMap<u64, usize>,
    dirty: bool,
}
use crate::{hash, Result, SceneWriter};

pub(crate) fn transform_shape(transform: &Transform, size_i: [u32; 3]) -> Transform {
    let (mut pos, mut rot) = transform.as_nalegbra_pair();
    let size = Vector3::from_iterator(size_i.iter().map(|&dim| (dim - dim % 2) as f32));
    // 0.05 m = half a voxel
    let axis_relative_offset = Vector3::new(0.05, 0.05, 0.0);
    let axis_offset = size.component_mul(&axis_relative_offset);
    pos += rot.transform_vector(&axis_offset);
    rot *= UnitQuaternion::from_axis_angle(&Vector3::x_axis(), TAU / 4.);
    (pos, rot).into()
}

impl StoreFile {
    fn new(path: PathBuf, palette: &[Material; 256]) -> Result<Self> {
        let mut shape_indices = HashMap::new();
        let vox = if path.exists() {
            let file = vox::semantic::parse_file(&path)?;
            for (i, child) in file.root.children().unwrap_or(&vec![]).iter().enumerate() {
                if let Some(name) = &child.name {
                    if let Ok(hash_n) = hash::str_to_n(name) {
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
            )?;
            file
        };
        Ok(Self {
            dirty: false,
            vox,
            shape_indices,
            path,
        })
    }

    pub(crate) fn store_voxel_data(&mut self, voxel_data: &Voxels) {
        let hash_n = hash::compute_n(&voxel_data);
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
                            pos: pos.into_fixed(),
                            index: palette_index,
                        });
                    }
                }
                let model = Model::new(voxel_data.size.map(|dim| dim.min(256)), voxels);
                #[allow(clippy::cast_possible_wrap)]
                let [x, y, z] = model.size().map(|x| (x as i32) / 2);
                let mut node = Node::new([x, y - 1, z], model);
                node.name = Some(hash::n_to_str(hash_n));
                self.vox.root.add(node);
                self.dirty = true;
                vacancy.insert(len);
            }
            Entry::Occupied(_) => {}
        }
    }

    fn write(&mut self) -> Result<()> {
        self.vox.clone().write(&mut File::create(&self.path)?)?;
        self.dirty = false;
        Ok(())
    }

    fn write_dirty(&mut self) -> Result<()> {
        if self.dirty {
            self.write()
        } else {
            Ok(())
        }
    }
}

impl Drop for StoreFile {
    fn drop(&mut self) {
        self.write_dirty().expect("while writing VoxStoreFile");
    }
}

impl Store {
    pub fn new<P: AsRef<Path>>(teardown_dir: P) -> Result<Arc<Mutex<Self>>> {
        let vox_dir = teardown_dir.as_ref().join("data").join("vox");
        if !vox_dir.exists() {
            bail!("data/vox didn't exist in teardown_dir")
        }
        Ok(Arc::new(Mutex::new(Self {
            hash_vox_dir: vox_dir.join("hash"),
            palette_files: HashMap::new(),
        })))
    }

    pub(crate) fn load_palettes(
        &mut self,
        palettes: &[&[Material; 256]],
    ) -> Result<Vec<Arc<Mutex<StoreFile>>>> {
        let mut vec = Vec::new();
        for palette in palettes {
            let hash_n = hash::compute_n(palette);
            vec.push(match self.palette_files.entry(hash_n) {
                Entry::Occupied(occupant) => occupant.get().clone(),
                Entry::Vacant(vacancy) => vacancy
                    .insert(Arc::new(Mutex::new(StoreFile::new(
                        self.hash_vox_dir
                            .join(format!("{}.vox", hash::n_to_str(hash_n))),
                        palette,
                    )?)))
                    .clone(),
            })
        }
        Ok(vec)
    }

    pub fn write_dirty(&mut self) -> Result<()> {
        for file in self.palette_files.values() {
            file.unwrap_lock().write_dirty()?;
        }
        Ok(())
    }
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
        if let Some(range) = range_for_material_kind(material_kind) {
            if index >= range[0] && index <= range[1] {
                return material_kind;
            }
        }
    }
    MaterialKind::None
}

fn range_for_material_kind(material_kind: MaterialKind) -> Option<[u8; 2]> {
    Some(match material_kind {
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
        MaterialKind::HardMetal => [169, 176],
        MaterialKind::HardMasonry => [177, 184],
        MaterialKind::Unknown13 => [185, 224],
        MaterialKind::Unphysical => [225, 240],
        MaterialKind::None => return None,
    })
}

#[derive(Debug)]
pub(crate) enum PaletteMapping<'a> {
    Original(&'a [Material; 256]),
    Remapped(Box<([Material; 256], [u8; 256])>),
}

impl PaletteMapping<'_> {
    pub(crate) fn materials_as_ref(&self) -> &[Material; 256] {
        match self {
            PaletteMapping::Original(original) => original,
            PaletteMapping::Remapped(remapped) => &remapped.0,
        }
    }
}

fn try_swap_index(
    i_u8: u8,
    orig_palette: &[Material; 256],
    new_to_orig: &mut [u8; 256],
    correct: &mut [bool; 256],
) -> Result<(), ()> {
    let i = i_u8 as usize;
    if correct[i] {
        Ok(())
    } else {
        let material = &orig_palette[new_to_orig[i] as usize];
        if let Some(range) = range_for_material_kind(material.kind) {
            for swap_i_u8 in range[0]..=range[1] {
                let swap_i = swap_i_u8 as usize;
                if !correct[swap_i] {
                    new_to_orig.swap(i, swap_i);
                    correct[swap_i] = true;
                    return Ok(());
                }
            }
            if !material.replacable {
                for swap_i_u8 in range[0]..=range[1] {
                    let swap_i = swap_i_u8 as usize;
                    if orig_palette[new_to_orig[swap_i] as usize].replacable {
                        correct[swap_i] = false;
                        new_to_orig.swap(i, swap_i);
                        correct[swap_i] = true;
                        return Ok(());
                    }
                }
            }
            Err(())
        } else {
            Ok(())
        }
    }
}

/// Rearrange materials of a palette so that the materials are at the correct
/// indices
#[allow(clippy::cast_possible_truncation)] // Never
pub(crate) fn remap_materials(orig_palette: &[Material; 256]) -> PaletteMapping {
    // let mut sneaky_palette = orig_palette.to_owned();
    // for material in sneaky_palette.iter_mut() {
    //     if material.replacable {
    //         *material = Material::default();
    //     }
    // }
    // let orig_palette = &sneaky_palette;
    let mut i_eq_value = [0_u8; 256];
    let mut correct = [false; 256];
    for i in 0..=255 {
        let i_u8 = i as u8;
        i_eq_value[i] = i_u8;
        correct[i] = material_kind_for_index(i_u8) == orig_palette[i].kind;
    }
    let mut new_to_orig = i_eq_value;
    let mut priority = i_eq_value;
    #[rustfmt::skip]
    priority.sort_unstable_by_key(|mat_i| match orig_palette[*mat_i as usize] {
        Material { kind: MaterialKind::None, replacable: true, .. } => 2,
        Material { replacable: true, .. } => 1,
        _ => 0,
    });
    let mut overflowed = Vec::new();
    for &i in &priority {
        if let Err(()) = try_swap_index(i, orig_palette, &mut new_to_orig, &mut correct) {
            let material = &orig_palette[new_to_orig[i as usize] as usize];
            if !material.replacable {
                overflowed.push(new_to_orig[i as usize])
            }
        }
    }
    if new_to_orig == i_eq_value {
        return PaletteMapping::Original(orig_palette);
    }
    let mut orig_to_new = i_eq_value;
    for (new, &orig) in new_to_orig.iter().enumerate() {
        orig_to_new[orig as usize] = new as u8;
    }
    let new_palette: [Material; 256] = (0..256_usize)
        .map(|i| orig_palette[new_to_orig[i] as usize].clone())
        .collect::<Vec<_>>()
        .into_fixed();
    if !overflowed.is_empty() {
        warn_wrong_indices(
            overflowed.as_ref(),
            &orig_palette,
            &new_palette,
            &orig_to_new,
        )
    }
    PaletteMapping::Remapped(Box::new((new_palette, orig_to_new)))
}

#[cold]
fn warn_wrong_indices(
    forced_filler_orig_indices: &[u8],
    orig_palette: &[Material; 256],
    new_palette: &[Material; 256],
    indices_orig_to_new: &[u8; 256],
) {
    println!(
        "Failed mappings in palette {}: {}",
        hash::n_to_str(hash::compute_n(&new_palette)),
        forced_filler_orig_indices
            .iter()
            .map(|orig_i| {
                let orig_i = *orig_i as usize;
                format!(
                    "{:?} -> {:?} ({})",
                    orig_palette[orig_i].kind,
                    material_kind_for_index(indices_orig_to_new[orig_i]),
                    indices_orig_to_new[orig_i],
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    )
}

pub(crate) fn convert_material(material: &Material) -> VoxMaterial {
    let Rgba([r, g, b, alpha]) = material.rgba;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let mut vox_mat =
        VoxMaterial::new_color([r, g, b, alpha].map(|comp| (comp * 255.).clamp(0., 255.) as u8));
    vox_mat.kind = if vox_mat.rgba[3] < 255 {
        vox_mat.alpha = Some(alpha);
        VoxMaterialKind::Glass
    } else if material.emission > 0.0 {
        let e = material.emission;
        let flux = if e > 1000.0 {
            eprintln!("emission {} too large for MagicaVoxel", e);
            4.
        } else if e > 100.0 {
            4.
        } else if e > 10.0 {
            3.
        } else if e > 1.0 {
            2.
        } else {
            1.
        };
        vox_mat.flux = Some(flux);
        vox_mat.emit = Some(e / 10_f32.powf(flux - 1.));
        VoxMaterialKind::Emit
    } else {
        vox_mat.metal = Some(material.metalness);
        vox_mat.rough = Some(1.0 - material.shinyness);
        vox_mat.spec = Some(material.reflectivity);
        vox_mat.weight = Some(1.0);
        VoxMaterialKind::Metal
    };
    vox_mat
}

impl SceneWriter<'_> {
    pub(crate) fn write_vox(&self) -> Result<(Vec<PaletteMapping<'_>>, HashMap<u32, Voxels<'_>>)> {
        fs::create_dir_all(&self.vox_store.unwrap_lock().hash_vox_dir)?;
        fs::create_dir_all(&self.mod_dir)?;
        if let Err(err) = fs::create_dir(&self.level_dir()) {
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
            let mut vox_store = self.vox_store.unwrap_lock();
            vox_store.load_palettes(
                palette_mappings.iter()
                    .map(PaletteMapping::materials_as_ref)
                    .collect::<Vec<_>>()
                    .as_ref(),)?
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
                        palette_file.unwrap_lock()
                            .store_voxel_data(&shape_voxel_data)
                    },
                )
            });
        Ok((palette_mappings, entity_voxels))
    }
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
