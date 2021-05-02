use std::{collections::HashMap, fs::File, io::Write};

use nalgebra::{Isometry3, Point3};
use quick_xml::{
    events::{BytesStart, Event},
    Writer,
};
use teardown_bin_format::{
    BoundaryVertex, Entity, EntityKind, Joint, JointKind, Shape, Tags, Voxels,
};

use crate::{
    hash, quaternion_to_euler,
    xml::attrs::{flatten, join_as_strings, ToXMLAttributes},
    PaletteMapping, Result, SceneWriter, WriteEntityContext, XMLResult,
};
pub mod attrs;

pub trait WriteXML {
    fn write_xml<W: Write>(&self, writer: &mut Writer<W>) -> XMLResult<()>;
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

impl SceneWriter<'_> {
    pub(crate) fn xml(
        &self,
        entity_voxels: HashMap<u32, Voxels>,
        palette_mappings: Vec<PaletteMapping>,
    ) -> Result<()> {
        let mut xml_file = File::create(self.mod_dir.join(format!("{}.xml", &self.name)))?;
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
                flatten(vec![
                    self.scene.spawnpoint.to_xml_attrs(),
                    vec![("name", "spawnpoint".to_string())]
                ]).iter().map(|(k, v)| (*k, v.as_ref())),),))?;
        #[rustfmt::skip]
        xml_writer.write_event(Event::Empty(
            BytesStart::owned_name("spawnpoint").with_attributes(
                flatten(vec![
                    self.scene.player.transform.to_xml_attrs(),
                    vec![("name", "player".to_string())]
                ]).iter().map(|(k, v)| (*k, v.as_ref())),),))?;
        self.scene.environment.write_xml(&mut xml_writer)?;
        Self::write_boundary(&self.scene.boundary_vertices, &mut xml_writer)?;
        xml_writer.write_event(Event::Empty(
            BytesStart::owned_name("script").with_attributes(vec![
                ("name", "turn off lights"),
                ("file", "lightsoff.lua"),
                ("param0", "global"),
            ]),
        ))?;
        let entities = self.scene.entities.iter().collect::<Vec<_>>();
        let mut write_entity_context = WriteEntityContext {
            entity_voxels,
            palette_mappings,
            scene: &self.scene,
            writer: &mut xml_writer,
        };
        for entity in entities {
            write_entity_context.write_entity_xml(entity, None, false)?;
        }
        xml_writer.write_event(Event::End(end))?;
        Ok(())
    }

    fn write_boundary(
        boundary: &[BoundaryVertex],
        writer: &mut Writer<&mut File>,
    ) -> XMLResult<()> {
        let start = BytesStart::owned_name("boundary").with_attributes(vec![("name", "the")]);
        let start_for_end = start.to_owned();
        writer.write_event(Event::Start(start))?;
        boundary.write_xml(writer)?;
        writer.write_event(Event::End(start_for_end.to_end()))?;
        Ok(())
    }
}

impl WriteEntityContext<'_, &mut File> {
    pub(crate) fn get_shape_name_and_xml_attrs(
        &self,
        entity: &Entity,
        shape: &Shape,
    ) -> (&'static str, Vec<(&'static str, String)>) {
        let mut kind_attrs = vec![
            (
                "texture",
                format!("{} {}", shape.texture_tile, shape.texture_weight),
            ),
            ("density", shape.density.to_string()),
            ("strength", shape.strength.to_string()),
        ];
        if shape.voxels.palette_index_runs.is_empty() {
            kind_attrs.push(("hidden_", true.to_string()))
        }
        if let Some(palette_mapping) = self.palette_mappings.get(shape.palette as usize) {
            kind_attrs.push((
                "file",
                format!(
                    "hash/{}.vox",
                    hash::n_to_str(hash::compute_n(palette_mapping.materials_as_ref()))
                ),
            ))
        } else {
            eprintln!("could not get palette mapping for {}", shape.palette);
        }
        if let Some(entity_voxels) = self.entity_voxels.get(&entity.handle) {
            kind_attrs.push(("object", hash::n_to_str(hash::compute_n(entity_voxels))));
        } else {
            eprintln!("could not get entity voxels for {}", entity.handle)
        }
        ("vox", kind_attrs)
    }

    pub(crate) fn joint_xml(&self, joint: &Joint) -> (&'static str, Vec<(&'static str, String)>) {
        if joint.kind == JointKind::Rope {
            ("rope", joint.to_xml_attrs())
        } else {
            let shape_handle = joint.shape_handles[0];
            let relative_pos = joint.shape_positions[0];
            let mut attrs = joint.to_xml_attrs();
            // FIXME: Inefficient
            if let Some(shape) = self.scene.iter_entities().find(|e| {
                matches!(e.kind, EntityKind::Body(_))
                    && e.children.iter().any(|child| child.handle == shape_handle)
            }) {
                #[allow(clippy::unwrap_used)]
                let isometry: Isometry3<f32> = shape.transform().unwrap().clone().into();
                let pos = isometry.transform_point(&Point3::new(
                    relative_pos[0],
                    relative_pos[1],
                    relative_pos[2],
                ));
                attrs.push(("pos", join_as_strings(pos.coords.iter())));
                attrs.push((
                    "rot",
                    join_as_strings(quaternion_to_euler(joint.ball_rot).iter()),
                ));
            }
            ("joint", attrs)
        }
    }
}

pub(crate) fn tag_to_string(tag: (&&str, &&str)) -> String {
    let (&k, &v) = tag;
    let mut s = k.to_string();
    if !v.is_empty() {
        s += "=";
        s += v;
    }
    s
}

pub(crate) fn tags_to_string(tags: &Tags) -> String {
    let mapped = tags.0.iter().map(tag_to_string);
    mapped.collect::<Vec<_>>().join(" ")
}
