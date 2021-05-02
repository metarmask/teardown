use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    convert::TryInto,
    fmt::{self, Formatter},
    hash::{Hash, Hasher},
    iter::{self, Copied, Filter, FlatMap, Repeat, Take, Zip},
    mem,
    slice::ArrayChunks,
};

use approx::{AbsDiffEq, RelativeEq};
use num_traits::PrimInt;
use structr::{Parse, ParseError, ParseErrorKind, Parser};

const VERSION: [u8; 3] = [0, 7, 1];

#[derive(Debug, Clone, Parse)]
pub struct Scene<'a> {
    #[structr(eq = "Scene::MAGIC")]
    magic: [u8; 5],
    #[structr(parse = "{ let v = parser.parse()?;
            if v != VERSION {
                println!(\"Warning. Version mismatch: {:?} != {:?}\", v, VERSION) } Ok(v) }")]
    pub version: [u8; 3],
    pub level: &'a str,
    pub z_bytes4_eq_0: [u8; 4],
    pub shadow_volume: [f32; 3],
    pub spawnpoint: Transform,
    pub player: Player,
    pub environment: Environment<'a>,
    pub z_f32_8: [f32; 8],
    pub z_u8: u8,
    #[structr(len = "u32")]
    pub boundary_vertices: Vec<BoundaryVertex>,
    #[structr(len = "u32")]
    pub fires: Vec<Fire>,
    #[structr(len = "u32")]
    pub palettes: Vec<Palette<'a>>,
    pub registry: Registry<'a>,
    #[structr(len = "u32")]
    pub entities: Vec<Entity<'a>>,
}

impl<'a> Scene<'a> {
    pub const MAGIC: &'static [u8] = &[0x54, 0x44, 0x42, 0x49, 0x4e];

    pub fn iter_entities(&'a self) -> impl Iterator<Item = &'a Entity> {
        self.entities.iter().flat_map(Entity::self_and_all_children)
    }
}

#[derive(Debug, Clone, Parse)]
pub struct Fire {
    pub entity_handle: u32,
    pub pos: [f32; 3],
    pub max_time: f32,
    pub time: f32,
    pub z_u8_6: [u8; 6],
}

pub mod light {
    use super::*;

    #[derive(Debug, Clone, Parse)]
    pub struct Light<'a> {
        pub z_u8_start: u8,
        pub kind: Kind,
        pub transform: Transform,
        pub rgba: Rgba,
        pub scale: f32,
        pub reach: f32,
        pub size: f32,
        pub unshadowed: f32,
        pub cone_angle: f32,
        pub cone_penumbra: f32,
        pub fog_iter: f32,
        pub fog_scale: f32,
        pub area_size: [f32; 2],
        pub z1_f32: f32,
        pub z_u8_17: [u8; 13],
        pub z2_f32: f32,
        pub sound: Sound<'a>,
        pub glare: f32,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Parse)]
    #[repr(u8)]
    pub enum Kind {
        Sphere = 1,
        Capsule = 2,
        Cone = 3,
        Area = 4,
    }
}
pub use light::{Kind as LightKind, Light};

pub mod joint {
    use super::*;

    #[derive(Debug, Clone, Parse)]
    pub struct Joint {
        pub kind: JointKind,
        pub shape_handles: [u32; 2],
        pub shape_positions: [[f32; 3]; 2],
        pub hinge_or_prismatic_rot: [[f32; 3]; 2],
        pub connected: bool,
        pub collisions: bool,
        pub rot_strength: f32,
        pub rot_spring: f32,
        pub ball_rot: [f32; 4],
        pub hinge_min_max: [f32; 2],
        pub z_f32_2: [f32; 2],
        pub size: f32,
        #[structr(
            parse = "Ok(if kind == JointKind::Rope { Some(parser.parse()?) } else { None })"
        )]
        pub rope: Option<Rope>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Parse)]
    #[repr(u32)]
    pub enum Kind {
        Ball = 1,
        Hinge = 2,
        Prismatic = 3,
        Rope = 4,
    }

    #[derive(Debug, Clone, Parse)]
    pub struct Rope {
        pub rgba: Rgba,
        pub float: f32,
        pub strength: f32,
        pub max_stretch: f32,
        pub z_f32_2: [f32; 2],
        pub z_u8: u8,
        #[structr(len = "u32")]
        pub knots: Vec<Knot>,
    }

    #[derive(Debug, Clone, Parse)]
    pub struct Knot {
        pub from: [f32; 3],
        pub to: [f32; 3],
    }
}
pub use joint::{Joint, Kind as JointKind, Knot, Rope};

#[derive(Debug, Default, Clone, Parse)]
pub struct Material {
    pub kind: MaterialKind,
    pub rgba: Rgba,
    pub reflectivity: f32,
    pub shinyness: f32,
    pub metalness: f32,
    pub emission: f32,
    pub replacable: bool,
}

impl Hash for Material {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
        self.rgba.hash(state);
        self.reflectivity.to_le_bytes().hash(state);
        self.shinyness.to_le_bytes().hash(state);
        self.metalness.to_le_bytes().hash(state);
        self.emission.to_le_bytes().hash(state);
        self.replacable.hash(state);
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Parse)]
#[repr(u8)]
pub enum MaterialKind {
    None = 0,
    Glass = 1,
    Wood = 2,
    /// Also known as concrete and brick
    Masonry = 3,
    Plaster = 4,
    /// Also known as weak metal
    Metal = 5,
    HeavyMetal = 6,
    Rock = 7,
    Dirt = 8,
    /// Also known as grass
    Foliage = 9,
    Plastic = 10,
    HardMetal = 11,
    HardMasonry = 12,
    Unknown13 = 13,
    Unphysical = 14,
}

impl Default for MaterialKind {
    fn default() -> Self {
        MaterialKind::None
    }
}

pub struct SelfAndChildrenIter<'a> {
    entity: &'a Entity<'a>,
    returned_self: bool,
    child_i: usize,
    child_children: Option<Box<SelfAndChildrenIter<'a>>>,
}

impl<'a> Iterator for SelfAndChildrenIter<'a> {
    type Item = &'a Entity<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.returned_self {
            self.child_children
                .as_mut()
                .and_then(Iterator::next)
                .or_else(|| {
                    self.entity.children.get(self.child_i).and_then(|child| {
                        self.child_i += 1;
                        self.child_children = Some(Box::new(child.self_and_all_children()));
                        #[allow(clippy::unwrap_used)]
                        self.child_children.as_mut().unwrap().next()
                    })
                })
        } else {
            self.returned_self = true;
            Some(self.entity)
        }
    }
}

#[derive(Debug, Clone, Parse)]
pub struct Entity<'a> {
    kind_byte: u8,
    pub handle: u32,
    pub tags: Tags<'a>,
    pub desc: &'a str,
    #[structr(parse = "EntityKind::parse(parser, kind_byte.into())")]
    pub kind: EntityKind<'a>,
    #[structr(len = "u32")]
    pub children: Vec<Entity<'a>>,
    #[structr(eq = "[0xef, 0xbe,0xef, 0xbeu8]")]
    beef_beef: [u8; 4],
}

impl<'a> Entity<'a> {
    #[must_use]
    pub fn transform(&self) -> Option<&Transform> {
        self.kind.transform()
    }
}

impl From<u8> for EntityKindVariants {
    fn from(byte: u8) -> Self {
        match byte {
            2 => Self::Shape,
            1 => Self::Body,
            10 => Self::Screen,
            5 => Self::Water,
            8 => Self::Vehicle,
            11 => Self::Trigger,
            4 => Self::Location,
            9 => Self::Wheel,
            7 => Self::Joint,
            12 => Self::Script,
            3 => Self::Light,
            // other => Self::Body,
            other => unimplemented!("entity {}", other),
        }
    }
}

impl<'a> Entity<'a> {
    #[must_use]
    pub fn self_and_all_children(&self) -> SelfAndChildrenIter<'_> {
        SelfAndChildrenIter {
            entity: &self,
            child_i: 0,
            returned_self: false,
            child_children: None,
        }
    }
}

#[derive(Debug, Clone, Parse)]
pub enum EntityKind<'a> {
    Shape(Shape<'a>),
    Body(Body),
    Screen(Screen<'a>),
    Water(Water),
    Vehicle(Vehicle<'a>),
    Trigger(Trigger<'a>),
    Location(Location),
    Wheel(Wheel<'a>),
    Joint(Joint),
    Script(Script<'a>),
    Light(Light<'a>),
}

impl<'a> EntityKind<'a> {
    #[must_use]
    pub fn transform(&self) -> Option<&Transform> {
        Some(match self {
            EntityKind::Shape(shape) => &shape.transform,
            EntityKind::Body(body) => &body.transform,
            EntityKind::Screen(screen) => &screen.transform,
            EntityKind::Water(water) => &water.transform,
            EntityKind::Vehicle(vehicle) => &vehicle.transform,
            EntityKind::Trigger(trigger) => &trigger.transform,
            EntityKind::Location(location) => &location.transform,
            EntityKind::Light(light) => &light.transform,
            /* EntityKind::Failed(_) | */
            EntityKind::Joint(_) | EntityKind::Wheel(_) | EntityKind::Script(_) => return None,
        })
    }

    #[must_use]
    pub fn z_u8_start(&self) -> u8 {
        *match self {
            EntityKind::Shape(shape) => &shape.z_u8_start,
            EntityKind::Body(body) => &body.z_u8_start,
            EntityKind::Screen(screen) => &screen.z_u8_start,
            EntityKind::Water(water) => &water.z_u8_start,
            EntityKind::Vehicle(vehicle) => &vehicle.z_u8_start,
            EntityKind::Trigger(trigger) => &trigger.z_u8_start,
            EntityKind::Location(location) => &location.z_u8_start,
            EntityKind::Joint(_) => &0,
            EntityKind::Light(light) => &light.z_u8_start,
            EntityKind::Wheel(wheel) => &wheel.z_u8_start,
            EntityKind::Script(script) => &script.z_u8_start,
        }
    }
}

#[derive(Debug, Clone, Parse)]
pub struct Exhaust {
    pub transform: Transform,
    // Values used in built-in levels: 0, 1.5, 2, 3
    pub z_f32: f32,
}

#[derive(Debug, Clone, Parse)]
pub struct Vehicle<'a> {
    pub z_u8_start: u8,
    pub body_handle: u32,
    pub transform: Transform,
    pub velocity: [f32; 3],
    pub angular_velocity: [f32; 3],
    pub z_f32_not_health: f32,
    #[structr(len = "u32")]
    pub wheel_handles: Vec<u32>,
    // Split off to help with compile times
    pub properties: VehicleProperties<'a>,
    pub z_f32_3: [f32; 3],
    pub player_pos: [f32; 3],
    pub z_f32_6: [f32; 6],
    pub difflock: f32,
    pub z6_f32_eq_1: f32,
    pub z_u32: u32,
    pub z2_u8: u8,
    pub z7_f32_eq_0: f32,
    #[structr(len = "u32")]
    pub refs: Vec<u32>,
    #[structr(len = "u32")]
    pub exhausts: Vec<Exhaust>,
    // pub what: [u8; 4],
    #[structr(len = "u32")]
    pub vitals: Vec<Vital>,
    #[structr(parse = "guess_arm_rot(parser)")]
    pub arm_rot: Option<f32>,
}

#[derive(Debug, Clone, Parse)]
pub struct VehicleProperties<'a> {
    /// In m/s
    pub max_speed: f32,
    pub z1_f32: f32,
    pub spring: f32,
    pub damping: f32,
    pub acceleration: f32,
    pub strength: f32,
    pub friction: f32,
    pub z2_f32: f32,
    pub z1_u8: u8,
    pub antispin: f32,
    pub steerassist: f32,
    // Possible value: 1.5
    pub z3_f32: f32,
    pub antiroll: f32,
    pub sound: VehicleSound<'a>,
}

fn guess_arm_rot<'p>(parser: &mut Parser<'p>) -> Result<Option<f32>, ParseError<'p>> {
    let i = parser.i;
    let hypothetical_length: u32 = parser.parse()?;
    parser.i = i;
    Ok(if hypothetical_length > 0 && hypothetical_length < 16 {
        None
    } else {
        Some(parser.parse()?)
    })
}

#[derive(Debug, Clone, Parse)]
pub struct Vital {
    pub body_handle: u32,
    pub z_f32: f32,
    pub pos: [f32; 3],
    pub shape_index: u32,
}

#[derive(Debug, Clone, Parse)]
pub struct VehicleSound<'a> {
    pub name: &'a str,
    pub pitch: f32,
}

#[derive(Debug, Clone, Parse)]
pub struct Water {
    pub z_u8_start: u8,
    pub transform: Transform,
    pub depth: f32,
    pub wave: f32,
    pub ripple: f32,
    pub motion: f32,
    pub foam: f32,
    #[structr(len = "u32")]
    pub boundary_vertices: Vec<BoundaryVertex>,
}

const N_TINTS: usize = 2;
const TINT_SHADES: usize = 4;
const PALETTE_SIZE: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TintKind {
    Black,
    Yellow,
}

#[derive(Clone, Parse)]
pub struct Palette<'a> {
    pub materials: [Material; PALETTE_SIZE],
    pub tint_tables: &'a [u8; N_TINTS * PALETTE_SIZE * TINT_SHADES],
    pub z_u8_eq_0: u8,
}

impl Palette<'_> {
    #[must_use]
    pub fn tinted_material(
        &self,
        index: u8,
        tint_kind: TintKind,
        extra_steps: u8,
    ) -> Option<&Material> {
        let tint_kind_offset = PALETTE_SIZE
            * TINT_SHADES
            * match tint_kind {
                TintKind::Black => 0,
                TintKind::Yellow => 1,
            };
        let i = self.tint_tables
            [tint_kind_offset + extra_steps as usize * TINT_SHADES + index as usize + 1];
        (i != 0).then_some(&self.materials[i as usize])
    }
}

impl fmt::Debug for Palette<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

#[allow(unused_qualifications)]
impl<'a> ::core::fmt::Display for Palette<'a> {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        let mut struct_ = f.debug_struct("Palette");
        struct_.field("materials", &self.materials);
        struct_.field("tint_tables", &(&self.tint_tables[0..8]));
        struct_.field("z_u8_eq_0", &self.z_u8_eq_0);
        struct_.finish()
    }
}

#[derive(Debug, Clone, Parse)]
pub struct Script<'a> {
    pub z_u8_start: u8,
    pub path: &'a str,
    pub params: Registry<'a>,
    pub last_update: f32,
    pub time: f32,
    pub z_u8_4: [u8; 4],
    pub table: LuaTable<'a>,
    #[structr(len = "u32")]
    pub entity_handles: Vec<u32>,
    #[structr(len = "u32")]
    pub sounds: Vec<ScriptSound<'a>>,
}

#[derive(Debug, Clone, Parse)]
pub struct Sound<'a> {
    pub path: &'a str,
    pub volume: f32,
}

pub mod environment {
    use super::*;

    #[derive(Debug, Clone, Parse)]
    pub struct Environment<'a> {
        pub skybox: Skybox<'a>,
        pub exposure: Exposure,
        pub fog: Fog,
        pub water: Water,
        pub nightlight: bool,
        pub ambience: Sound<'a>,
        pub slippery: f32,
        pub lights_fog_scale: f32,
    }

    #[derive(Debug, Clone, Parse)]
    pub struct Skybox<'a> {
        pub texture: &'a str,
        pub color_intensity: Rgba,
        /// In radians
        pub rotation: f32,
        pub sun: Sun,
        pub z_u8: u8,
        pub constant: Rgba,
        pub ambient_light: f32,
        pub ambient_exposure: f32,
    }

    #[derive(Debug, Clone, Parse)]
    pub struct Sun {
        pub tint_brightness: [f32; 3],
        pub tint: Rgba,
        pub direction: [f32; 3],
        pub brightness: f32,
        pub spread: f32,
        pub max_shadow_length: f32,
        pub fog_scale: f32,
        pub glare: f32,
    }

    #[derive(Debug, Clone, Parse)]
    pub struct Fog {
        pub color: Rgba,
        pub start: f32,
        pub distance: f32,
        pub amount: f32,
        pub exponent: f32,
    }

    #[derive(Debug, Clone, Parse)]
    pub struct Water {
        pub wetness: f32,
        pub puddle_coverage: f32,
        pub puddle_size: f32,
        pub rain: f32,
    }
}
pub use environment::Environment;

#[derive(Debug, Clone, Parse)]
pub struct Trigger<'a> {
    pub z_u8_start: u8,
    pub transform: Transform,
    pub type_: TriggerGeometryKind,
    pub sphere_radius: f32,
    pub half_cuboid: [f32; 3],
    pub polygon_extrusion: f32,
    #[structr(len = "u32")]
    pub polygon_vertices: Vec<BoundaryVertex>,
    pub sound: TriggerSound<'a>,
}

#[derive(Debug, Clone, Parse)]
pub struct TriggerSound<'a> {
    pub path: &'a str,
    pub ramp: f32,
    pub byte: u8,
    pub volume: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Parse)]
#[repr(u32)]
pub enum TriggerGeometryKind {
    Sphere = 1,
    Box = 2,
    Polygon = 3,
}

#[derive(Debug, Clone, Parse)]
pub struct Body {
    pub z_u8_start: u8,
    pub transform: Transform,
    pub velocity: [f32; 3],
    pub angular_velocity: [f32; 3],
    pub dynamic: bool,
    pub active: bool,
    pub z_u8: u8,
}

#[derive(Debug, Clone, Parse)]
pub struct Wheel<'a> {
    pub z_u8_start: u8,
    pub z_u8_108: &'a [u8; 108],
}

#[derive(Debug, Clone, Parse)]
pub struct Exposure {
    pub min: f32,
    pub max: f32,
    pub brightness_goal: f32,
}

#[derive(Debug, Clone, Parse)]
pub struct BoundaryVertex {
    pub x: f32,
    pub z: f32,
}

#[derive(Debug, Clone)]
pub struct Registry<'a>(pub HashMap<&'a str, &'a str>);

impl<'p> Parse<'p> for Registry<'p> {
    fn parse<'a>(parser: &'a mut Parser<'p>) -> Result<Self, ParseError<'p>>
    where 'p: 'a {
        let n: u32 = parser.parse()?;
        let n_strings = n as usize * 2;
        let entries: Vec<&'p str> = parser.parse_n(n_strings)?;
        let mut map = HashMap::new();
        for [key, value] in entries.array_chunks() {
            map.insert(*key, *value);
        }
        Ok(Registry(map))
    }
}

#[derive(Debug, Clone, Parse)]
pub struct Location {
    pub z_u8_start: u8,
    pub transform: Transform,
}

#[derive(Clone, Parse)]
pub struct Rgba(pub [f32; 4]);

impl Rgba {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    #[must_use]
    pub fn u8(&self) -> [u8; 4] {
        self.0.map(|channel| (channel * 255.) as u8)
    }
}

impl Hash for Rgba {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.u8().hash(state);
    }
}

impl fmt::Debug for Rgba {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "#")?;
        for channel in &self.u8() {
            write!(f, "{:02x}", channel)?;
        }
        Ok(())
    }
}

impl Default for Rgba {
    fn default() -> Self {
        Rgba([0., 0., 0., 1.])
    }
}

#[derive(Clone, Parse)]
pub struct Rgb(pub [f32; 3]);

impl Rgb {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    #[must_use]
    pub fn u8(&self) -> [u8; 3] {
        self.0.map(|channel| (channel * 255.) as u8)
    }
}

impl fmt::Debug for Rgb {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "#")?;
        for channel in &self.u8() {
            write!(f, "{:02x}", channel)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Parse)]
pub struct Player {
    pub z_i32_3: [i32; 3],
    pub z_f32: [f32; 7],
    pub transform: Transform,
    pub yaw: f32,
    pub pitch: f32,
    pub velocity: [f32; 3],
    pub health: f32,
    pub z_f32_2: [f32; 2],
}

#[derive(Clone, PartialEq, Parse)]
pub enum LuaValue<'a> {
    Boolean(bool),
    Number(f64),
    Table(LuaTable<'a>),
    String(&'a str),
}

// Taken from derive(Hash), but modified to take hash bytes of double.
impl<'a> Hash for LuaValue<'a> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            LuaValue::Boolean(v) => {
                mem::discriminant(self).hash(state);
                v.hash(state);
            }
            LuaValue::Number(v) => {
                mem::discriminant(self).hash(state);
                v.to_le_bytes().hash(state);
            }
            LuaValue::Table(v) => {
                mem::discriminant(self).hash(state);
                v.hash(state);
            }
            LuaValue::String(v) => {
                mem::discriminant(self).hash(state);
                v.hash(state);
            }
        }
    }
}

// Lua tables do not allow NaN as keys
impl<'a> Eq for LuaValue<'a> {}

impl fmt::Debug for LuaValue<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let dbg: &dyn fmt::Debug = match self {
            LuaValue::Boolean(ref v) => v,
            LuaValue::Number(ref v) => v,
            LuaValue::Table(ref v) => v,
            LuaValue::String(ref v) => v,
        };
        dbg.fmt(f)
    }
}

impl<'p> Parse<'p> for LuaValue<'p> {
    fn parse<'a>(parser: &'a mut Parser<'p>) -> Result<Self, ParseError<'p>>
    where 'p: 'a {
        Ok(match parser.parse::<u32>()? {
            1 => LuaValue::Boolean(parser.parse()?),
            // TODO: Replace error kind
            2 => return Err(Parser::error(ParseErrorKind::NoReprIntMatch(2))),
            3 => LuaValue::Number(parser.parse()?),
            4 => LuaValue::String(parser.parse()?),
            5 => LuaValue::Table(parser.parse()?),
            other => {
                return Err(Parser::error(ParseErrorKind::NoReprIntMatch(u64::from(
                    other,
                ))))
            }
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LuaTable<'a>(HashMap<LuaValue<'a>, LuaValue<'a>>);

impl<'a> Hash for LuaTable<'a> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for entry in &self.0 {
            entry.hash(state);
        }
    }
}

impl<'p> Parse<'p> for LuaTable<'p> {
    fn parse<'a>(parser: &'a mut Parser<'p>) -> Result<Self, ParseError<'p>>
    where 'p: 'a {
        let mut entries = HashMap::new();
        loop {
            let i = parser.i;
            let lua_type: u32 = parser.parse()?;
            if lua_type == 0 {
                return Ok(LuaTable(entries));
            }
            parser.i = i;
            entries.insert(parser.parse()?, parser.parse()?);
        }
    }
}

#[derive(Debug, Clone, Parse)]
pub struct ScriptSound<'a> {
    pub kind: ScriptSoundKind,
    pub name: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Parse)]
#[repr(u32)]
pub enum ScriptSoundKind {
    Normal = 1,
    Loop = 2,
    Unknown3 = 3,
}

#[derive(Debug, Clone, Parse)]
pub struct Screen<'a> {
    pub z_u8_start: u8,
    pub transform: Transform,
    pub size: [f32; 2],
    pub bulge: f32,
    pub resolution: [u32; 2],
    pub script: &'a str,
    pub enabled: u8,
    pub interactive: u8,
    pub emissive: f32,
    pub fx_chromatic_aberration: f32,
    pub fx_noise: f32,
    pub fx_glitch: f32,
    pub z_u8_4: [u8; 4],
}

#[derive(Debug, Clone)]
pub struct Tags<'a>(pub HashMap<&'a str, &'a str>);

impl<'p> Parse<'p> for Tags<'p> {
    fn parse<'a>(parser: &'a mut Parser<'p>) -> Result<Self, ParseError<'p>>
    where 'p: 'a {
        let n: u8 = parser.parse()?;
        let n_strings = n as usize * 2;
        let entries: Vec<&'p str> = parser.parse_n(n_strings)?;
        let mut map = HashMap::new();
        for [key, value] in entries.array_chunks() {
            map.insert(*key, *value);
        }
        Ok(Tags(map))
    }
}

#[derive(Debug, Clone, Parse, PartialEq)]
pub struct Transform {
    /// x, y, z
    pub pos: [f32; 3],
    /// x, y, z, w
    pub rot: [f32; 4],
}

impl Default for Transform {
    fn default() -> Self {
        Transform {
            pos: [0., 0., 0.],
            rot: [0., 0., 0., 1.],
        }
    }
}

#[cfg(feature = "convert_nalgebra")]
mod convert_nalgebra {
    use nalgebra::{Isometry3, Point3, Quaternion, UnitQuaternion};

    use super::*;
    impl Transform {
        #[must_use]
        pub fn as_nalegbra_pair(&self) -> (Point3<f32>, UnitQuaternion<f32>) {
            (
                Point3::from_slice(&self.pos),
                UnitQuaternion::from_quaternion(Quaternion::from_parts(
                    self.rot[3],
                    Point3::from_slice(&self.rot[0..3]).coords,
                )),
            )
        }
    }

    impl From<Transform> for Isometry3<f32> {
        fn from(transform: Transform) -> Isometry3<f32> {
            Isometry3 {
                translation: Point3::from_slice(&transform.pos).coords.into(),
                rotation: UnitQuaternion::from_quaternion(Quaternion::from_parts(
                    transform.rot[3],
                    Point3::from_slice(&transform.rot[0..3]).coords,
                )),
            }
        }
    }

    impl From<Isometry3<f32>> for Transform {
        fn from(isometry: Isometry3<f32>) -> Self {
            Transform {
                pos: isometry.translation.vector.into(),
                rot: {
                    let rot = isometry.rotation;
                    let w = rot.w;
                    let x = rot.i;
                    let y = rot.j;
                    let z = rot.k;
                    [x, y, z, w]
                },
            }
        }
    }

    impl From<(Point3<f32>, UnitQuaternion<f32>)> for Transform {
        fn from((pos, rot): (Point3<f32>, UnitQuaternion<f32>)) -> Self {
            Transform {
                pos: pos.coords.into(),
                rot: {
                    let w = rot.w;
                    let x = rot.i;
                    let y = rot.j;
                    let z = rot.k;
                    [x, y, z, w]
                },
            }
        }
    }
}

const TOLERANCE_ADJUSTMENT: f32 = 4.;

impl AbsDiffEq for Transform {
    type Epsilon = <f32 as AbsDiffEq>::Epsilon;

    fn default_epsilon() -> Self::Epsilon {
        f32::default_epsilon() * TOLERANCE_ADJUSTMENT
    }

    fn abs_diff_eq(&self, other: &Self, epsilon: Self::Epsilon) -> bool {
        self.pos
            .iter()
            .chain(self.rot.iter())
            .zip(other.pos.iter().chain(other.rot.iter()))
            .all(|(a, b)| a.abs_diff_eq(b, epsilon))
    }
}

impl RelativeEq for Transform {
    fn default_max_relative() -> Self::Epsilon {
        f32::default_max_relative() * TOLERANCE_ADJUSTMENT
    }

    fn relative_eq(
        &self,
        other: &Self,
        epsilon: Self::Epsilon,
        max_relative: Self::Epsilon,
    ) -> bool {
        self.pos
            .iter()
            .chain(self.rot.iter())
            .zip(other.pos.iter().chain(other.rot.iter()))
            .all(|(a, b)| a.relative_eq(b, epsilon, max_relative))
    }
}

#[derive(Debug, Clone, Parse)]
pub struct Shape<'a> {
    pub z_u8_start: u8,
    pub transform: Transform,
    pub z_u8_4: [u8; 4],
    pub density: f32,
    pub strength: f32,
    pub texture_tile: u32,
    // Texture offset?
    pub starting_world_position: [f32; 3],
    pub texture_weight: f32,
    pub z_f32: f32,
    pub z1_u8: u8,
    pub z2_u8: u8,
    pub voxels: Voxels<'a>,
    pub palette: u32,
    pub voxel_scaling: f32,
    // Most commonly ff. Also common: all 00. Only two cases of: fffff00
    pub z_i32_3: [i32; 2],
    pub z3_u8: u8,
}

impl<'a> Shape<'a> {
    #[must_use]
    pub fn iter_voxels(&'a self) -> VoxelIter<'a> {
        self.voxels.iter()
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct Voxels<'a> {
    pub size: [u32; 3],
    /// Pairs of (n-1,  palette index)
    pub palette_index_runs: Cow<'a, [u8]>,
}

impl<'p> Parse<'p> for Voxels<'p> {
    fn parse<'a>(parser: &'a mut Parser<'p>) -> Result<Self, ParseError<'p>>
    where 'p: 'a {
        let size: [u32; 3] = parser.parse()?;
        let volume = size[0] * size[1] * size[2];
        Ok(if volume == 0 {
            Self {
                size,
                palette_index_runs: Cow::Borrowed(&[]),
            }
        } else {
            let n = parser.parse::<u32>()? as usize;
            Self {
                size,
                palette_index_runs: Cow::Borrowed(parser.take_dynamically(n)?),
            }
        })
    }
}

pub struct BoxIter<I>
where I: PrimInt
{
    size: [I; 3],
    current: [I; 3],
    order: [usize; 3],
    done: bool,
}

impl<I> Iterator for BoxIter<I>
where I: PrimInt + std::ops::AddAssign
{
    type Item = [I; 3];

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let to_return = Some(self.current);
        for &dim_i in &self.order {
            self.current[dim_i] += I::one();
            if self.current[dim_i] >= self.size[dim_i] {
                self.current[dim_i] = I::zero();
            } else {
                return to_return;
            }
        }
        self.done = true;
        to_return
    }
}

impl<I> BoxIter<I>
where I: PrimInt
{
    /// # Panics
    /// Panics if `order` is not the set {0, 1, 2}
    pub fn new(size: [I; 3], order: [usize; 3]) -> BoxIter<I> {
        assert_eq!(
            order.iter().copied().collect::<HashSet<_>>(),
            (0..3).collect()
        );
        BoxIter {
            size,
            order,
            current: [I::zero(); 3],
            done: false,
        }
    }
}

impl<'a> Voxels<'a> {
    #[must_use]
    pub fn iter(&'a self) -> VoxelIter<'a> {
        VoxelIter::new(self)
    }
}

#[allow(clippy::type_complexity)]
pub struct VoxelIter<'a>(
    Filter<
        Zip<
            BoxIter<i32>,
            FlatMap<
                Copied<ArrayChunks<'a, u8, 2>>,
                Take<Repeat<u8>>,
                fn([u8; 2]) -> Take<Repeat<u8>>,
            >,
        >,
        fn(&([i32; 3], u8)) -> bool,
    >,
);

fn flat_map_voxel_data_chunk([n_times, palette_index]: [u8; 2]) -> Take<Repeat<u8>> {
    iter::repeat(palette_index).take(n_times as usize + 1)
}

impl<'a> VoxelIter<'a> {
    fn new(voxel_data: &'a Voxels<'a>) -> Self {
        Self::new_from_parts(&voxel_data.size, voxel_data.palette_index_runs.as_ref())
    }

    #[allow(clippy::cast_possible_wrap)]
    fn new_from_parts(size: &'a [u32; 3], compressed_palette_indices: &'a [u8]) -> Self {
        VoxelIter(
            BoxIter::new(size.map(|dim| dim as i32), [0, 1, 2])
                .zip(
                    compressed_palette_indices
                        .array_chunks::<2>()
                        .copied()
                        .flat_map(flat_map_voxel_data_chunk as fn([u8; 2]) -> Take<Repeat<u8>>),
                )
                .filter(|(_, palette_index)| *palette_index != 0),
        )
    }
}

impl<'a> Iterator for VoxelIter<'a> {
    type Item = ([i32; 3], u8);

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl fmt::Debug for Voxels<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("VoxelData")
            .field("size", &self.size)
            .field(
                "compressed_voxel_indices",
                &self.palette_index_runs[0..usize::min(8, self.palette_index_runs.len())].to_vec(),
            )
            .finish()
    }
}
