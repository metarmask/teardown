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
    vox::{transform_shape, PaletteMapping},
    xml::{
        attrs::{join_as_strings, ToXMLAttributes},
        WriteXML,
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
                dynamic = body.dynamic == 1;
                ("body", body.to_xml_attrs())
            }
            EntityKind::Shape(shape) => self.get_shape_name_and_xml_attrs(entity, shape),
            EntityKind::Script(script) => ("script", script.to_xml_attrs()),
            EntityKind::Vehicle(vehicle) => ("vehicle", vehicle.to_xml_attrs()),
            EntityKind::Wheel(_) => ("wheel", vec![]),
            EntityKind::Joint(joint) => self.joint_xml(joint),
            EntityKind::Light(light) => ("light", light.to_xml_attrs()),
            EntityKind::Location(_) => ("location", vec![]),
            EntityKind::Screen(_) => ("screen", vec![]),
            EntityKind::Trigger(_) => ("trigger", vec![]),
            EntityKind::Water(water) => ("water", water.to_xml_attrs()),
        };
        let start = BytesStart::owned_name(name);
        let mut attrs = Vec::new();
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
                for pos in [knots.first().map(|knot| knot.from), knots.last().map(|knot| knot.to),].iter().flatten() {
                    self.writer.write_event(Event::Empty(
                        BytesStart::owned_name("location")
                            .with_attributes(vec![("pos", join_as_strings(pos.iter()).as_ref())]),
                    ))?;
                }
            },
            _ => {}
        }
        self.writer.write_event(Event::End(end))?;
        Ok(())
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
