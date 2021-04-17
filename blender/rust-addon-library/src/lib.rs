#![feature(array_map, array_chunks)]
use std::{
    collections::{hash_map::DefaultHasher, BTreeMap, HashMap, HashSet},
    f32::consts::PI,
    hash::{Hash, Hasher},
};

use building_blocks::{core::Axis3Permutation, mesh::OrientedCubeFace, storage::access::GetMut};
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressIterator, ProgressStyle};
use pyo3::{exceptions, prelude::*, types::PyDict, wrap_pyfunction};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use teardown_bin_format::{
    light::Kind as LightKind, Entity, EntityKind, EntityKindVariants, Light, Material,
    MaterialKind, Palette, Rgba, Scene, Shape, Transform,
};

struct ImportContext<'a> {
    py: Python<'a>,
    palette_materials: HashMap<u32, HashMap<u8, &'a PyAny>>,
    hash_material_map: HashMap<u64, &'a PyAny>,
    progress_style: ProgressStyle,
    entity_progress: ProgressBar,
    new_light: &'a PyAny,
    new_mesh: &'a PyAny,
    new_object: &'a PyAny,
    new_collection: &'a PyAny,
    new_camera: &'a PyAny,
    view_layer: &'a PyAny,
    material_template: &'a PyAny,
}

/// Polygons all have the same amount of edges
///
/// No way to specify loops
struct BlenderMeshSpec {
    verts: Vec<f32>,
    edges: Option<Vec<i32>>,
    polygon_loop_total: i32,
    polygon_vert_indices: Vec<i32>,
    polygon_material_index: Option<Vec<i16>>,
}

impl Default for BlenderMeshSpec {
    fn default() -> Self {
        Self {
            polygon_loop_total: 4,
            verts: Vec::new(),
            edges: None,
            polygon_vert_indices: Vec::new(),
            polygon_material_index: None,
        }
    }
}

impl BlenderMeshSpec {
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    fn apply_to_mesh(self, mesh: &PyAny, py: Python) -> PyResult<()> {
        let py_verts = mesh.getattr("vertices")?;
        let py_loops = mesh.getattr("loops")?;
        let py_polygons = mesh.getattr("polygons")?;
        assert_eq!(self.verts.len() % 3, 0);
        let n_verts = self.verts.len() as i32 / 3;
        py_verts.call_method1("add", (n_verts,))?;
        let n_polygons = self.polygon_vert_indices.len() as i32 / self.polygon_loop_total;
        py_loops.call_method1("add", (n_polygons * self.polygon_loop_total,))?;
        py_polygons.call_method1("add", (n_polygons,))?;
        py_verts.call_method1("foreach_set", ("co", self.verts))?;
        let mut update_loose_edges = false;
        if let Some(edges) = self.edges {
            assert_eq!(edges.len() as i32, n_verts / 3 * 2);
            let py_edges = mesh.getattr("edges")?;
            py_edges.call_method1("add", (edges.len() / 2,))?;
            py_edges.call_method1("foreach_set", ("vertices", edges))?;
            update_loose_edges = true;
        }
        {
            let mut loop_totals = Vec::new();
            let mut loop_starts = Vec::new();
            for i in 0..n_polygons {
                loop_totals.push(self.polygon_loop_total);
                loop_starts.push(i * self.polygon_loop_total);
            }
            py_polygons.call_method1("foreach_set", ("loop_total", loop_totals))?;
            py_polygons.call_method1("foreach_set", ("loop_start", loop_starts))?;
        }
        assert_eq!(
            self.polygon_vert_indices.len() as i32 % self.polygon_loop_total,
            0
        );
        py_polygons.call_method1("foreach_set", ("vertices", self.polygon_vert_indices))?;
        if let Some(polygon_material_index) = self.polygon_material_index {
            py_polygons.call_method1("foreach_set", ("material_index", polygon_material_index))?;
        }
        let dict = PyDict::new(py);
        // Also calculates loops, so always neccessary
        dict.set_item("calc_edges", true)?;
        if update_loose_edges {
            dict.set_item("calc_edges_loose", true)?;
        }
        mesh.call_method("update", (), Some(dict))?;
        // mesh.call_method0("validate")?;
        Ok(())
    }
}

pub fn compute_hash_n<H: Hash>(to_hash: &H) -> u64 {
    let mut hasher = DefaultHasher::new();
    to_hash.hash(&mut hasher);
    hasher.finish()
}

fn get_entity_name(entity: &Entity) -> String {
    let mut s = String::new();
    if !entity.desc.is_empty() {
        s += &(entity.desc.to_owned() + " ");
    }
    s += &format!(
        "{} {:?}",
        entity.handle,
        EntityKindVariants::from(&entity.kind)
    );
    s
}

fn set_transform(obj: &PyAny, transform: Option<&Transform>) -> PyResult<()> {
    if let Some(Transform {
        pos,
        rot: [x, y, z, w],
    }) = transform
    {
        obj.setattr("location", (pos[0], pos[1], pos[2]))?;
        obj.setattr("rotation_mode", "QUATERNION")?;
        obj.setattr("rotation_quaternion", (*w, *x, *y, *z))?;
    }
    Ok(())
}

impl<'a> ImportContext<'a> {
    fn create_mesh_for_shape(shape: &Shape, palettes: &[Palette]) -> BlenderMeshSpec {
        let (mut palette_indices, quads) = shape.to_mesh(palettes);
        let mut vert_position_indices: BTreeMap<[i32; 3], i32> = BTreeMap::new();
        let mut polygon_vert_indices: Vec<i32> = Vec::new();
        let mut polygon_material_index: Vec<i16> = Vec::new();
        let mut vert_i = 0;
        for quad_group in &quads.quad_groups {
            for quad in &quad_group.quads {
                let corners = quad_group.face.quad_corners(quad);
                let corner_indices: [i32; 4] = corners.map(|corner| {
                    *vert_position_indices.entry(corner.0).or_insert_with(|| {
                        let old = vert_i;
                        vert_i += 1;
                        old
                    })
                });

                // edges.extend([ 0, 2,  2, 3,  3, 1,  1, 0 ].iter().map(|rel_i|
                // corner_indices[*rel_i]));
                let OrientedCubeFace {
                    permutation,
                    n_sign,
                    ..
                } = quad_group.face;
                polygon_vert_indices.extend(
                    if if permutation == Axis3Permutation::ZXY {
                        -1
                    } else {
                        1
                    } * n_sign
                        == 1
                    {
                        [2, 3, 1, 0]
                    } else {
                        [0, 1, 3, 2]
                    }
                    .iter()
                    .map(|rel_i| corner_indices[*rel_i]),
                );
                polygon_material_index.push(i16::from(palette_indices.get_mut(quad.minimum).0));
            }
        }
        let verts: Vec<f32> = {
            let mut map_as_vec = vert_position_indices.into_iter().collect::<Vec<_>>();
            map_as_vec.sort_unstable_by_key(|(_, index)| *index);
            let mut verts = Vec::new();
            for (pos, _) in map_as_vec {
                for dim in &pos {
                    verts.push((*dim) as f32 * 0.1);
                }
            }
            verts
        };
        BlenderMeshSpec {
            verts,
            edges: None,
            polygon_loop_total: 4,
            polygon_vert_indices,
            polygon_material_index: Some(polygon_material_index),
        }
    }

    fn create_object(
        &mut self,
        entity: &Entity,
        collection: &'a PyAny,
        parsed: &Scene,
        meshes: &mut HashMap<u32, BlenderMeshSpec>,
    ) -> PyResult<&PyAny> {
        self.entity_progress.inc(1);
        let mut obj: Option<&PyAny> = None;
        let mut obj_data: Option<&PyAny> = None;
        match &entity.kind {
            EntityKind::Light(Light {
                kind,
                cone_angle,
                cone_penumbra,
                rgba,
                size,
                ..
            }) => {
                let name = format!("{} {:?}", get_entity_name(entity), kind);
                let light;
                match kind {
                    LightKind::Area => {
                        light = self.new_light.call1((name, "AREA"))?;
                    }
                    LightKind::Sphere | LightKind::Capsule => {
                        light = self.new_light.call1((name, "POINT"))?;
                        light.setattr("color", (rgba.0[0], rgba.0[1], rgba.0[2]))?;
                    }
                    LightKind::Cone => {
                        light = self.new_light.call1((name, "SPOT"))?;
                        light.setattr("spot_size", cone_angle)?;
                        light.setattr("spot_blend", cone_penumbra / cone_angle)?;
                    }
                }
                light.setattr("energy", 100)?;
                light.setattr("shadow_soft_size", size)?;
                obj_data = Some(light);
            }
            EntityKind::Shape(shape) => {
                let blender_mesh = self
                    .new_mesh
                    .call1((format!("{} mesh", get_entity_name(entity)),))?;
                let mesh_obj = self
                    .new_object
                    .call1((get_entity_name(entity), blender_mesh))?;
                if shape.voxels.size.iter().any(|&dim| dim == 0) {
                    println!("Weird thing: {:?}", entity);
                }
                if let Some(mesh) = meshes.remove(&entity.handle) {
                    if mesh.polygon_material_index.as_ref().unwrap().len() > 100 {
                        let dict = PyDict::new(self.py);
                        dict.set_item("view_layer", self.view_layer)?;
                        mesh_obj.call_method("hide_set", (false,), Some(dict))?;
                    }
                    mesh.apply_to_mesh(blender_mesh, self.py)?;
                }
                mesh_obj.setattr("texture_tile", shape.texture_tile)?;
                mesh_obj.setattr("texture_weight", shape.texture_weight)?;
                let s = shape.voxel_scaling * 10.;
                mesh_obj.setattr("scale", (s, s, s))?;
                let mesh_materials = blender_mesh.getattr("materials")?;
                let mut needs_default_material = true;
                let palette = shape.palette;
                if let Some(palette_materials) = self.palette_materials.get(&palette) {
                    let mut none_buffer = Vec::new();
                    for i in 0..255 {
                        if let Some(&material) = palette_materials.get(&i) {
                            none_buffer.push(Some(material));
                            for material in none_buffer {
                                needs_default_material = false;
                                mesh_materials.call_method1("append", (material,))?;
                            }
                            none_buffer = Vec::new();
                        } else {
                            none_buffer.push(None);
                        }
                    }
                }
                if needs_default_material {
                    mesh_materials.call_method1("append", (self.material_template,))?;
                    mesh_materials.call_method1("append", (self.material_template,))?;
                }
                obj = Some(mesh_obj);
            }
            _ => {}
        }
        let obj = if let Some(obj) = obj {
            obj
        } else {
            self.new_object.call1((get_entity_name(entity), obj_data))?
        };
        set_transform(obj, entity.kind.transform())?;
        collection
            .getattr("objects")?
            .getattr("link")?
            .call1((obj,))?;
        for child in &entity.children {
            let child_obj = self.create_object(child, collection, parsed, meshes)?;
            child_obj.setattr("parent", obj)?;
        }
        Ok(obj)
    }

    #[allow(clippy::cast_possible_truncation)]
    fn create_palette(&mut self, palette_i: usize, palette: &Palette) {
        let mut index_map: HashMap<u8, &'a PyAny> = HashMap::new();
        for (material_i, material) in palette.materials.iter().enumerate() {
            if let MaterialKind::None = material.kind {
                continue;
            }
            let hash = compute_hash_n(material);
            if let Some(blend_mat) = self.hash_material_map.get(&hash) {
                index_map.insert(material_i as u8, blend_mat);
            }
        }
        self.palette_materials.insert(palette_i as u32, index_map);
    }

    fn import(&mut self, path: &str) -> PyResult<Py<PyAny>> {
        let uncompressed = teardown_bin_format::read_to_uncompressed(path)?;
        let parsed = teardown_bin_format::parse_uncompressed(&uncompressed)
            .map_err(|err| PyErr::new::<exceptions::PyException, _>(format!("{:?}", err)))?;
        let mut n_all_entities = 0_usize;
        let shapes: Vec<(&Entity, &Shape)> = parsed
            .iter_entities()
            .filter_map(|entity| {
                n_all_entities += 1;
                match &entity.kind {
                    EntityKind::Shape(shape) => Some((entity, shape)),
                    _ => None,
                }
            })
            .collect();
        let palette_progress = ProgressBar::new(parsed.palettes.len() as u64);
        palette_progress.set_style(self.progress_style.clone());
        palette_progress.set_message("Palettes");
        // FIXME: Convoluted and inefficient
        // Determines which materials are actually in use. Identical materials
        // count as one. Later used to construct a map for each palette, mapping indices
        // to the shared Blender materials which were previously constructed.
        // This is used when the shape is added.
        let mut pal_i_map = HashMap::new();
        let mut hash_to_palette = HashMap::new();
        let mut material_set_indices = HashSet::new();
        for (palette_i, palette) in parsed.palettes.iter().enumerate() {
            for (material_i, material) in palette.materials.iter().enumerate() {
                if matches!(material.kind, MaterialKind::None) {
                    continue;
                }
                let hash = compute_hash_n(material);
                #[allow(clippy::cast_possible_truncation)]
                pal_i_map.insert((palette_i, material_i as u8), hash);
                hash_to_palette.entry(hash).or_insert(material);
            }
        }
        for (entity, shape) in &shapes {
            if parsed.palettes.get(shape.palette as usize).is_some() {
                for (_, mat_i) in shape.iter_voxels() {
                    material_set_indices.insert((shape.palette as usize, mat_i));
                }
            } else {
                eprintln!(
                    "Warning: Invalid palette reference in entity {}",
                    entity.handle
                );
            }
        }
        let mut material_set = HashSet::new();
        for indices in material_set_indices {
            if let Some(hash) = pal_i_map.get(&indices) {
                material_set.insert(*hash);
            }
        }
        for hash in material_set {
            let material = hash_to_palette.get(&hash).unwrap();
            let blender_mat = self.material_template.call_method0("copy")?;
            blender_mat.setattr("name", format!("{:?}:{:02}", material.kind, hash))?;
            let sliders = blender_mat
                .getattr("node_tree")?
                .getattr("nodes")?
                .get_item(0)?
                .getattr("inputs")?;
            let Material {
                rgba: Rgba([r, g, b, alpha]),
                shinyness,
                metalness,
                reflectivity,
                emission,
                ..
            } = material;
            sliders
                .get_item(0)?
                .setattr("default_value", (r, g, b, 1.0))?;
            for (i, value) in [alpha, shinyness, metalness, reflectivity, emission]
                .iter()
                .enumerate()
            {
                sliders.get_item(i + 1)?.setattr("default_value", **value)?;
            }
            self.hash_material_map.insert(hash, blender_mat);
        }
        for (i, palette) in parsed
            .palettes
            .iter()
            .enumerate()
            .progress_with(palette_progress)
        {
            self.create_palette(i, palette);
        }
        let new_collection = self
            .new_collection
            .call1((format!("{} (Teardown level)", parsed.level),))?;

        self.entity_progress.set_style(self.progress_style.clone());
        self.entity_progress.set_length(n_all_entities as u64);
        self.entity_progress.set_message("Entities");
        let shape_progress = ProgressBar::new(shapes.len() as u64);
        shape_progress.set_style(self.progress_style.clone());
        shape_progress.set_message("Shape mesh preparation");
        let mut shape_meshes = shapes
            .par_iter()
            .progress_with(shape_progress)
            .map(|(entity, shape)| {
                (
                    entity.handle,
                    Self::create_mesh_for_shape(&shape, &parsed.palettes),
                )
            })
            .collect::<HashMap<_, _>>();

        // Just the scene children
        for entity in &parsed.entities {
            self.create_object(entity, new_collection, &parsed, &mut shape_meshes)?;
        }
        let player_camera = self.new_camera.call1(("Player camera camera",))?;
        let tau = PI * 2.;
        player_camera.setattr("angle", tau / 4.)?;
        player_camera.setattr("lens_unit", "FOV")?;
        player_camera.setattr("passepartout_alpha", 1)?;
        let player_camera_obj = self.new_object.call1(("Player camera", player_camera))?;
        let pos = parsed.player.transform.pos;
        player_camera_obj.setattr("location", (pos[0], pos[1] + 1.7, pos[2]))?;
        player_camera_obj.setattr("rotation_mode", "XYZ")?;
        player_camera_obj.setattr(
            "rotation_euler",
            (parsed.player.yaw, parsed.player.pitch, 0),
        )?;
        println!("player: {:?}", parsed.player);
        new_collection
            .getattr("objects")?
            .getattr("link")?
            .call1((player_camera_obj,))?;
        Ok(new_collection.into())
    }
}

#[pyfunction]
fn import_as_collection(py: Python, path: &str) -> PyResult<Py<PyAny>> {
    let bpy = py.import("bpy")?;
    let bpy_data = bpy.getattr("data")?;
    let progress_style = ProgressStyle::default_bar()
        .progress_chars("█▏▎▍▌▋▊▉ ")
        .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}");
    let entity_progress = ProgressBar::new_spinner();
    entity_progress.set_style(progress_style.clone());
    ImportContext {
        palette_materials: HashMap::new(),
        hash_material_map: HashMap::new(),
        entity_progress: ProgressBar::new(0),
        progress_style,
        py,
        new_light: bpy_data.getattr("lights")?.getattr("new")?,
        new_mesh: bpy_data.getattr("meshes")?.getattr("new")?,
        material_template: bpy_data
            .getattr("materials")?
            .call_method1("get", ("Teardown template",))?,
        new_object: bpy_data.getattr("objects")?.getattr("new")?,
        new_collection: bpy_data.getattr("collections")?.getattr("new")?,
        new_camera: bpy_data.getattr("cameras")?.getattr("new")?,
        view_layer: bpy.getattr("context")?.getattr("view_layer")?,
    }
    .import(path)
}

#[pymodule]
fn libteardown_import(_: Python, m: &PyModule) -> PyResult<()> {
    // pyo3_log::init();
    m.add_function(wrap_pyfunction!(import_as_collection, m)?)?;
    Ok(())
}
