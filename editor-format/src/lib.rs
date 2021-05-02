#![feature(array_map, array_chunks, stmt_expr_attributes)]
mod hash;
mod xml;
// Public
pub mod util;
pub mod vox;

#[cfg(test)]
mod tests;

use std::{
    collections::HashMap,
    f32::consts::TAU,
    fmt::Debug,
    fs::File,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use ::vox::semantic::SemanticError as VoxError;
use anyhow::Result;
use derive_builder::Builder;
use nalgebra::{Isometry3, Point3, Quaternion, UnitQuaternion};
pub(crate) use quick_xml::Result as XMLResult;
use quick_xml::{
    events::{BytesStart, Event},
    Writer,
};
use teardown_bin_format::{
    Entity, EntityKind, EntityKindVariants, Joint, Rope, Scene, Transform, Voxels,
};
use thiserror::Error;

use crate::{
    util::IntoFixedArray,
    vox::{transform_shape, PaletteMapping},
    xml::{
        attrs::{join_as_strings, ToXMLAttributes},
        tags_to_string, WriteXML,
    },
};

#[derive(Debug, Error)]
enum Error {
    #[error(".vox error: {:#}", 0)]
    Vox(#[from] VoxError),
    #[error("xml error: {:#}", 0)]
    Xml(#[from] quick_xml::Error),
    #[error("Wheel entity did not have exactly one child: {:?}", 0)]
    SingleWheelChild(String),
}

#[derive(Builder)]
pub struct SceneWriter<'a> {
    scene: &'a Scene<'a>,
    mod_dir: PathBuf,
    vox_store: Arc<Mutex<vox::Store>>,
    #[builder(default = "\"main\".into()")]
    name: String,
}

impl SceneWriter<'_> {
    pub fn write_scene(&self) -> Result<()> {
        let (entity_voxels, palette_mappings) = self.write_vox()?;
        self.xml(palette_mappings, entity_voxels)?;
        Ok(())
    }

    fn level_dir(&self) -> PathBuf {
        self.mod_dir.join(&self.name)
    }
}

pub(crate) struct WriteEntityContext<'a, W: Write> {
    palette_mappings: Vec<PaletteMapping<'a>>,
    entity_voxels: HashMap<u32, Voxels<'a>>,
    scene: &'a Scene<'a>,
    writer: &'a mut Writer<W>,
}

impl WriteEntityContext<'_, &mut File> {
    pub fn write_entity_xml(
        &mut self,
        entity: &Entity,
        parent: Option<&Entity>,
        mut dynamic: bool,
    ) -> Result<()> {
        // debug_write_entity_positions(entity, parent);
        let mut tags = entity.tags.clone();
        let (name, mut kind_attrs) = match &entity.kind {
            EntityKind::Body(body) => {
                #[rustfmt::skip]
                // Skip the body in wheels, and write the shape inside directly
                if matches!(parent, Some(Entity { kind: EntityKind::Wheel(_), .. })) {
                    if entity.children.len() != 1 {
                        return Err(Error::SingleWheelChild(format!("{:?}", entity)).into());
                    }
                    return self.write_entity_xml(&entity.children[0], Some(entity), dynamic);
                }
                dynamic = body.dynamic;
                ("body", body.to_xml_attrs())
            }
            EntityKind::Shape(shape) => self.get_shape_name_and_xml_attrs(entity, shape),
            EntityKind::Script(script) => ("script", script.to_xml_attrs()),
            EntityKind::Vehicle(vehicle) => ("vehicle", vehicle.to_xml_attrs()),
            EntityKind::Wheel(_) => ("wheel", vec![]),
            EntityKind::Joint(joint) => self.joint_xml(joint),
            EntityKind::Light(light) => {
                if !light.on {
                    tags.0.insert("turnoff", "");
                }

                ("light", light.to_xml_attrs())
            }
            EntityKind::Location(_) => ("location", vec![]),
            EntityKind::Screen(_) => ("screen", vec![]),
            EntityKind::Trigger(_) => ("trigger", vec![]),
            EntityKind::Water(water) => ("water", water.to_xml_attrs()),
        };
        let start = BytesStart::owned_name(name);
        let mut attrs = vec![("name", self.name_entity(entity))];
        if let Some(mut world_transform) = corrected_transform(Some(entity)) {
            // If parent body is dynamic, then light is relative to shape in the save
            // representation
            if let Some(parent_transform) = corrected_transform(parent) {
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
        attrs.append(&mut entity.to_xml_attrs());
        if !tags.0.is_empty() {
            attrs.push(("tags", tags_to_string(&tags)));
        }
        attrs.append(&mut kind_attrs);
        let start = start.with_attributes(attrs.iter().map(|(k, v)| (*k, v.as_ref())));
        let end = start.to_end().into_owned();
        self.writer.write_event(Event::Start(start))?;
        for child in &entity.children {
            self.write_entity_xml(child, Some(entity), dynamic)?;
        }
        match &entity.kind {
            EntityKind::Water(water) => {
                water.boundary_vertices.as_slice().write_xml(self.writer)?;
            }
            #[rustfmt::skip]
            EntityKind::Joint(Joint { rope: Some(Rope { knots, .. }), .. }) => {
                let mut write_loc = |name: &str, pos: &[f32; 3]| {
                    self.writer.write_event(Event::Empty(
                        BytesStart::owned_name("location")
                            .with_attributes(vec![("name", name), ("pos", join_as_strings(pos.iter()).as_str())]),
                    ))
                };
                if knots.len() >= 2 {
                    write_loc("from", &knots[0].from)?;
                    write_loc("to", &knots[knots.len()-1].to)?;
                    let between = &knots[1..knots.len()-1];
                    for knot in between {
                        let average = knot.from.iter().zip(knot.to.iter()).map(|(from, to)| (from + to) / 2.0).collect::<Vec<_>>().into_fixed();
                        write_loc("between", &average)?;
                    }
                }
            }
            _ => {}
        }
        self.writer.write_event(Event::End(end))?;
        Ok(())
    }

    fn is_flashlight(&self, entity: &Entity) -> bool {
        let last_entity = self.scene.entities.last();
        last_entity.map_or(false, |last| last.handle == entity.handle)
    }

    fn name_entity(&self, entity: &Entity) -> String {
        let mut parts = vec![entity.handle.to_string()];
        match &entity.kind {
            EntityKind::Shape(shape) => {
                parts.push(format!("{} voxels", shape.voxels.iter().count()))
            }
            EntityKind::Body(body) => {
                if !body.dynamic {
                    parts.push("static".into())
                }
            }
            EntityKind::Screen(_) | EntityKind::Trigger(_) | EntityKind::Wheel(_) => {}
            EntityKind::Water(water) => {
                parts.push(format!("{} m deep", water.depth));
            }
            EntityKind::Vehicle(vehicle) => {
                if !vehicle.properties.sound.name.is_empty() {}
                parts.push(vehicle.properties.sound.name.into())
            }
            EntityKind::Location(_) => parts.push(tags_to_string(&entity.tags)),
            EntityKind::Joint(joint) => parts.push(format!("{:?}", joint.kind).to_lowercase()),
            EntityKind::Script(script) => {
                let short_path = script
                    .to_xml_attrs()
                    .into_iter()
                    .find_map(|(k, v)| if k == "file" { Some(v) } else { None })
                    .unwrap_or_default();
                parts.push(
                    short_path
                        .strip_suffix(".lua")
                        .unwrap_or(&short_path)
                        .into(),
                )
            }
            EntityKind::Light(light) => {
                if self.is_flashlight(entity) {
                    parts.push("flashlight".into());
                }
                parts.push(format!("{:?}", light.kind).to_lowercase())
            }
        }
        parts.join(" ")
    }
}

#[allow(dead_code)]
fn debug_write_entity_positions(entity: &Entity, parent: Option<&Entity>) {
    println!(
        "{:>8} {:<8}: {:+05.1?} {:+05.1?}", //  {:+05.1?}
        format!("{:?}", EntityKindVariants::from(&entity.kind)),
        entity.tags.0.iter().next().map_or("", |tag| tag.0),
        entity.transform().map(ToOwned::to_owned).map(|mut x| {
            x.pos = x.pos.map(|dim| dim * 10.);
            x
        }),
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

pub(crate) fn corrected_transform(parent: Option<&Entity>) -> Option<Transform> {
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

/// YZX euler angles
fn quaternion_to_euler(rot: [f32; 4]) -> [f32; 3] {
    let m = UnitQuaternion::from_quaternion(Quaternion::from_parts(
        rot[3],
        Point3::from_slice(&rot[0..3]).coords,
    ))
    .to_rotation_matrix();
    if m[(1, 0)] < 1.0 {
        if m[(1, 0)] > -1.0 {
            [
                f32::atan2(-m[(1, 2)], m[(1, 1)]),
                f32::atan2(-m[(2, 0)], m[(0, 0)]),
                f32::asin(m[(1, 0)]),
            ]
        } else {
            [0.0, -f32::atan2(m[(2, 1)], m[(2, 2)]), -TAU / 4.0]
        }
    } else {
        [0.0, f32::atan2(m[(2, 1)], m[(2, 2)]), TAU / 4.0]
    }
    .map(f32::to_degrees)
}
