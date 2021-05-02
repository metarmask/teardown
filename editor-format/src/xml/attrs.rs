//! Conversion for simple attributes that do not require context or extensive
//! calculations

use std::{
    self,
    io::Write,
    iter,
    path::{Path, PathBuf},
};

pub(crate) use quick_xml::Result as XMLResult;
use quick_xml::{
    events::{BytesStart, Event},
    Writer,
};
use teardown_bin_format::{
    environment::{self, Fog, Skybox, Sun},
    Body, Entity, Environment, Exposure, Joint, JointKind, Light, LightKind, Rope, Script, Sound,
    Transform, Vehicle, Water, Wheel,
};

use crate::{quaternion_to_euler, xml::WriteXML};

pub trait ToXMLAttributes {
    fn to_xml_attrs(&self) -> Vec<(&'static str, String)>;
}

pub fn flatten(deep_attrs: Vec<Vec<(&'static str, String)>>) -> Vec<(&'static str, String)> {
    let mut flattened = Vec::new();
    for mut attrs in deep_attrs {
        flattened.append(&mut attrs);
    }
    flattened
}

pub fn join_as_strings<I: IntoIterator<Item = U>, U: ToString>(iter: I) -> String {
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

impl ToXMLAttributes for Entity<'_> {
    fn to_xml_attrs(&self) -> Vec<(&'static str, String)> {
        let mut attrs = Vec::new();
        if !self.desc.is_empty() {
            attrs.push(("desc", self.desc.to_owned()));
        }
        attrs
    }
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
        flatten(vec![
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
                flatten(vec![
                    self.skybox.to_xml_attrs(),
                    self.exposure.to_xml_attrs(),
                    self.fog.to_xml_attrs(),
                    self.water.to_xml_attrs(),
                    vec![
                        ("name", "the".into()),
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
        vec![
            ("pos", join_as_strings(self.pos.iter())),
            ("rot", join_as_strings(quaternion_to_euler(self.rot).iter())),
        ]
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
            (
                "color",
                join_as_strings(self.rgba.0.iter().map(|c| c.powf(0.45_45_45)).take(3)),
            ),
            ("scale", self.scale.to_string()),
            (
                "angle",
                (f32::acos(self.cone_angle).to_degrees() * 2.0).to_string(),
            ),
            (
                "penumbra",
                ((f32::acos(self.cone_angle) - f32::acos(self.cone_penumbra)).to_degrees() * 2.0)
                    .to_string(),
            ),
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
            ("topspeed", (props.max_speed * 3.6).to_string()),
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
        vec![("dynamic", self.dynamic.to_string())]
    }
}

impl ToXMLAttributes for Joint {
    fn to_xml_attrs(&self) -> Vec<(&'static str, String)> {
        if let Some(rope) = &self.rope {
            flatten(vec![
                vec![("size", self.size.to_string())],
                rope.to_xml_attrs(),
            ])
        } else {
            vec![
                (
                    "type",
                    match self.kind {
                        JointKind::Ball => "ball",
                        JointKind::Hinge => "hinge",
                        JointKind::Prismatic => "prismatic",
                        JointKind::Rope => unreachable!(),
                    }
                    .to_string(),
                ),
                ("size", self.size.to_string()),
                ("rotstrength", self.rot_strength.to_string()),
                ("rotspring", self.rot_spring.to_string()),
                (
                    "limits",
                    join_as_strings({
                        let mut limits = self.limits;
                        if let JointKind::Hinge = self.kind {
                            for angle in &mut limits {
                                *angle = f32::to_degrees(*angle)
                            }
                        }
                        limits.to_vec().iter()
                    }),
                ),
                ("collide", self.collisions.to_string()),
                // ("sound", .to_string())
            ]
        }
    }
}

impl ToXMLAttributes for Rope {
    fn to_xml_attrs(&self) -> Vec<(&'static str, String)> {
        vec![
            ("color", join_as_strings(self.rgba.0.iter())),
            ("strength", self.strength.to_string()),
            ("slack", self.float.to_string()),
            ("maxstretch", self.max_stretch.to_string()),
        ]
    }
}

impl ToXMLAttributes for Wheel {
    fn to_xml_attrs(&self) -> Vec<(&'static str, String)> {
        vec![
            ("drive", self.drive_factor.to_string()),
            ("steer", self.steer_factor.to_string()),
            ("travel", join_as_strings(self.suspension_range.iter())),
        ]
    }
}
