#![feature(stmt_expr_attributes, iter_intersperse, backtrace, backtrace_frames)]
use owning_ref::OwningHandle;
use rlua::{Lua, FromLua, prelude::LuaError};
#[cfg(custom_after_load)]
mod after_load;
#[cfg(custom_after_load)]
use after_load::after_load;
#[cfg(feature = "graphical")]
mod graphical;

use std::{
    collections::{HashSet, HashMap},
    ffi::OsString,
    fs, iter,
    path::{Path, PathBuf},
};

use keyvalues_parser::{Value, Obj, Vdf};
use ::vox as vox_format;
use anyhow::{Context, Result, bail};
use clap::ArgGroup;
#[cfg(feature = "graphical")]
use iced::{Application, Settings};
use structopt::StructOpt;
use teardown_bin_format::parse_file;
use teardown_editor_format::{vox, SceneWriterBuilder, SceneWriterBuilderError};
use thiserror::Error;
use Error::UnexpectedVDF as VDFErr;

#[derive(Debug, Error)]
enum Error {
    #[cfg(target_os = "windows")]
    #[error("Unable to get the path to the main Steam directory from the registry")]
    SteamPathInRegistry,
    #[cfg(not(target_os = "windows"))]
    #[error("No home directory found")]
    NoHomeDir,
    #[error("Steam dir not found at {0}")]
    NoMainSteamDir(PathBuf),
    #[error("Steam app {0} not found")]
    SteamAppNotFound(String),
    #[error("Unexpected type/value when reading a Valve KeyValues file (sometimes .vdf)")]
    UnexpectedVDF,
    #[error("Could not build the scene writer: {:#}", 0)]
    SceneWriterBuild(SceneWriterBuilderError),
    #[cfg(feature = "graphical")]
    #[error("Could not initialize iced GUI: {0}")]
    IcedInit(String),
    #[error("No Steam install directory")]
    NoSteamInstallDir,
    #[error("Error while processing Lua")]
    Lua(#[from] LuaError),
}

#[derive(StructOpt)]
enum AfterLoadCmd {
    Convert {
        #[structopt(default_value = "converted")]
        mod_name: String,
        #[structopt(default_value = "main")]
        level_name: String,
    },
    PrintEnv,
    #[cfg(custom_after_load)]
    IgnoredCode,
}

#[derive(StructOpt)]
enum Subcommand {
    ShowVox {
        vox_file: PathBuf,
    },
    ConvertAll {
        teardown_folder: PathBuf,
        mods_folder: PathBuf,
    },
    /// Loads a level. Defaults to quicksave.
    Load {
        #[structopt(flatten)]
        bin_select: BinSelect,
        #[structopt(subcommand)]
        then: AfterLoadCmd,
    },
}

#[derive(Default, StructOpt)]
#[structopt(group(ArgGroup::with_name("bin-select").args(&["path", "name", "quicksave"]).required(true)))]
struct BinSelect {
    #[structopt(long, short = "i")]
    path: Option<PathBuf>,
    #[structopt(long, short)]
    name: Option<String>,
    #[structopt(long, short)]
    quicksave: bool,
}

#[derive(StructOpt)]
struct Options {
    #[structopt(subcommand)]
    command: Option<Subcommand>,
}

fn level_name_from_path<P: AsRef<Path>>(path: P) -> String {
    let mut path = path.as_ref().to_owned();
    path.set_extension("");
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

fn read_lua_with_includes<P1: AsRef<Path>, P2: AsRef<Path>>(file: P1, search_dir: P2, included: &mut HashSet<PathBuf>) -> Result<String> {
    let file_content = std::fs::read_to_string(search_dir.as_ref().join(&file))?;
    let mut full_file = String::new();
    for line in file_content.lines() {
        if let Some(s) = line.strip_prefix("#include \"").and_then(|a| a.strip_suffix('\"')) {
            let import_path = PathBuf::from(s);
            if !included.contains(&import_path) {
                full_file += &read_lua_with_includes(&import_path, search_dir.as_ref(), included)?;
                included.insert(import_path);
            }
        } else {
            full_file += line;
            full_file += "\n";
        }
    }
    Ok(full_file)
}

pub fn print_lua_value(v: rlua::Value, depth: usize) -> String {
    if depth == 0 { return "too deep".to_string() }
    type RLuaValue<'a> = rlua::Value<'a>;
    match v {
        RLuaValue::Nil => "nil".to_string(),
        RLuaValue::Boolean(v) => format!("{}", v),
        RLuaValue::LightUserData(_) => "light user data".to_string(),
        RLuaValue::Integer(v) => format!("{}", v),
        RLuaValue::Number(v) => format!("{}", v),
        RLuaValue::String(v) => format!("{:?}", v.to_str().unwrap_or("err")),
        RLuaValue::Table(table) => {
            let mut s = "{".to_owned();
            for kv in table.pairs::<RLuaValue, RLuaValue>() {
                let (k, v) = kv.unwrap();
                s += &format!("{}: {},", print_lua_value(k, depth - 1), print_lua_value(v, depth - 1))
            }
            s += "}";
            s
        },
        RLuaValue::Function(_) => "func".to_string(),
        RLuaValue::Thread(_) => "thread".to_string(),
        RLuaValue::UserData(_) => "user data".to_string(),
        RLuaValue::Error(_) => "error".to_string(),
    }
    
}

pub(crate) type StrToStr = StrTo<String>;
pub(crate) type StrTo<T> = HashMap<String, T>;

pub(crate) struct GameLuaMeta {
    levels: StrTo<StrToStr>,
    missions: StrTo<StrToStr>,
    sandbox: StrTo<StrToStr>,
    cinematic_parts: StrTo<Vec<StrToStr>>,
    challenges: StrTo<StrToStr>,
}

pub(crate) fn load_level_meta() -> Result<GameLuaMeta> {
    let lua = Lua::new();
    let full_code = read_lua_with_includes("game.lua", "/home/metarmask/.local/share/Steam/steamapps/common/Teardown/data/", &mut Default::default())?;
    lua.context(|lua_ctx| {
        lua_ctx
            .load(&full_code)
            .set_name("example code")?
            .exec()?;
        let globals = lua_ctx.globals();
        let cinematic_raw_values: StrTo<rlua::Table> = globals.get("gCinematic")?;
        let mut cinematic_parts: StrTo<Vec<StrToStr>> = HashMap::new();
        for (k, v) in cinematic_raw_values {
            if let Some(parts) = v.get("parts").context("no parts on cinematic")? {
                cinematic_parts.insert(k, parts);
            }
        }
        let mut missions = StrTo::<StrToStr>::new();
        for (k, v) in globals.get::<_, StrTo<StrTo<rlua::Value>>>("gMissions")? {
            let mut mission = StrToStr::new();
            for (k2, v2) in v {
                if let rlua::Value::Integer(_) | rlua::Value::Number(_) | rlua::Value::String(_) = v2 {
                    mission.insert(k2, FromLua::from_lua(v2, lua_ctx)?);
                }
            }
            missions.insert(k, mission);
        }
        let mut sandbox = StrTo::<StrToStr>::new();
        for level in globals.get::<_, Vec<StrToStr>>("gSandbox")? {
            sandbox.insert(level.get("id").context("no id on sandbox")?.to_string(), level);
        }
        Ok(GameLuaMeta {
            cinematic_parts, missions, sandbox,
            levels: globals.get("gLevels")?,
            challenges: globals.get("gChallenges")?,
        })
    })
}

#[allow(clippy::too_many_lines)]
fn main() -> Result<()> {
    let args = Options::from_args();
    let command = if let Some(command) = args.command {
        command
    } else {
        #[cfg(feature = "graphical")]
        graphical::App::run(Settings::default())
            .map_err(|iced_error| Error::IcedInit(format!("{:#}", iced_error)))?;

        #[cfg(not(feature = "graphical"))]
        println!("The binary was not compiled with the graphical feature");
        return Ok(());
    };

    match command {
        Subcommand::ShowVox { vox_file } => {
            let semantic = vox_format::syntax::parse_file(vox_file);
            println!("{:?}", semantic);
        }
        Subcommand::ConvertAll {
            teardown_folder,
            mods_folder,
        } => {
            let data_folder = teardown_folder.join("data");
            let mut created_mods = HashSet::new();
            let vox_store = vox::Store::new(teardown_folder)?;
            for file in fs::read_dir(data_folder.join("bin"))? {
                let file = file?;
                println!("Reading {}", file.file_name().to_string_lossy());
                let scene = parse_file(file.path())?;
                let registry = &scene.registry.0;
                let mut level_path = PathBuf::from(
                    registry
                        .get("game.levelpath")
                        .expect("levels should have game.levelpath registry entry"),
                );
                level_path.set_extension("");
                // Example: lee
                let level_name = level_path
                    .file_name()
                    .map_or_else(|| "what".into(), ToOwned::to_owned);
                // Example: lee_tower
                let &level_id = registry
                    .get("game.levelid")
                    .expect("levels should have game.levelid registry entry");
                #[rustfmt::skip] if level_id.is_empty() { continue; }
                let mod_dir = mods_folder.join(&level_name);
                #[rustfmt::skip] if !created_mods.insert(level_name) { continue; }
                SceneWriterBuilder::default()
                    .vox_store(vox_store.clone())
                    .mod_dir(mod_dir)
                    .scene(&scene)
                    .build()
                    .map_err(Error::SceneWriterBuild)?
                    .write_scene()?;
            }
            for mod_ in &created_mods {
                fs::write(mods_folder.join(mod_).join("main.xml"), "")?;
                #[rustfmt::skip]
                fs::write(
                    mods_folder.join(mod_).join("info.txt"),
                    format!(
                        "name = {}\
                        author = Tuxedo Labs\
                        description = ",
                        mod_.to_string_lossy()))?;
            }
        }
        Subcommand::Load { then, bin_select } => {
            let dirs = find_teardown_dirs()?;
            let scene = parse_file(match bin_select {
                BinSelect {
                    path: Some(file), ..
                } => file,
                BinSelect {
                    name: Some(name), ..
                } => {
                    #[rustfmt::skip]
                    let level_paths = fs::read_dir(dirs.main.join("data").join("bin"))?
                        .map(|res| {
                            res.map(|dir_entry| {
                                let path = dir_entry.path();
                                (level_name_from_path(&path), path)})})
                        .collect::<Result<Vec<_>, _>>()?;
                    #[rustfmt::skip]
                    level_paths.into_iter()
                        .find(|(other_name, _)| &name == other_name).context("No level with that name")?
                        .1
                }
                BinSelect {
                    quicksave: true, ..
                } => dirs.progress.join("quicksave.bin"),
                _ => unreachable!("because of arg group"),
            })?;
            match then {
                AfterLoadCmd::Convert {
                    mod_name,
                    level_name,
                } => {
                    #[rustfmt::skip]
                        SceneWriterBuilder::default()
                            .vox_store(vox::Store::new(dirs.main)?)
                            .mod_dir(dirs.mods.join(mod_name))
                            .name(level_name)
                            .scene(&scene)
                            .build().map_err(Error::SceneWriterBuild)?
                            .write_scene()?;
                }
                AfterLoadCmd::PrintEnv => {
                    println!("{:#?}", scene.environment);
                }
                #[cfg(custom_after_load)]
                AfterLoadCmd::IgnoredCode => {
                    after_load(scene)?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn get_steam_dir_path_through_registry() -> Result<PathBuf> {
    let registry_key = registry::Hive::CurrentUser
        .open(r"Software\Valve\Steam", registry::Security::Read)
        .context(Error::SteamPathInRegistry)?;
    let path = registry_key
        .value("SteamPath")
        .context(Error::SteamPathInRegistry)?;
    if let registry::Data::String(path) = path {
        Ok(path.to_os_string().into())
    } else {
        Err(anyhow::format_err!("Invalid registry data type")).context(Error::SteamPathInRegistry)
    }
}

#[cfg(target_os = "windows")]
fn get_steam_dir_path() -> Result<PathBuf> {
    get_steam_dir_path_through_registry().or_else(|err| {
        eprintln!("{}", err);
        Ok(r"C:\Program Files (x86)\Steam\".into())
    })
}

#[cfg(not(target_os = "windows"))]
fn get_steam_dir_path() -> Result<PathBuf> {
    let home_dir = dirs::home_dir().ok_or(Error::NoHomeDir)?;
    Ok(home_dir.join(".local/share/Steam"))
}

fn get_steam_dir() -> Result<PathBuf> {
    let path = get_steam_dir_path()?;
    if path.exists() {
        Ok(path)
    } else {
        Err(Error::NoMainSteamDir(path).into())
    }
}

fn get_extra_library_dirs(main_dir: &Path) -> Result<Vec<PathBuf>> {
    let library_dirs_path = main_dir.join(
        ["steamapps", "libraryfolders.vdf"].iter().collect::<PathBuf>());
    let vdf_string = std::fs::read_to_string(library_dirs_path)?;
    let vdf = Vdf::parse(&vdf_string)?;
    let entries = vdf.value.get_obj().context(VDFErr)?.iter();
    let mut libraries = Vec::new();
    for (k, values) in entries {
        if k.parse::<i32>().is_ok() {
            for library_dir in values {
                match library_dir {
                    Value::Str(str) => {
                        libraries.push(str.clone().into_owned().into());
                    },
                    Value::Obj(obj) => {
                        for path in obj.get("path").context(VDFErr)? {
                            libraries.push(path.get_str().context(VDFErr)?.into());
                        }
                    },
                }
            }
        }
    }
    Ok(libraries)
}

fn get_steam_library_dirs() -> Result<Vec<PathBuf>> {
    let main_dir = get_steam_dir()?;
    let extras = get_extra_library_dirs(&main_dir)
        .context("Could not get extra library folders")
        .unwrap_or_else(|err| {
            eprintln!("{:?}", err);
            Vec::new()
        });
    Ok(iter::once(main_dir).chain(extras).collect())
}

struct SteamApp<'a> {
    manifest: Obj<'a>,
    library_path: PathBuf,
}

fn single<T, I: IntoIterator<Item = T>>(iter: I) -> Option<T> {
    let mut iter = iter.into_iter();
    let first = iter.next();
    if iter.next().is_some() { return None }
    first
}

fn single_str<'a, 'b: 'a, 'c: 'b, I: 'a + IntoIterator<Item = &'b Value<'c>>>(values: I) -> Option<&'a str> {
    single(values)?.get_str()
}

fn single_obj<'a, 'b: 'a, 'c: 'b, I: 'a + IntoIterator<Item = &'b Value<'c>>>(values: I) -> Option<&'a Obj<'a>> {
    single(values)?.get_obj()
}

impl SteamApp<'_> {
    fn compat_drive(&self) -> Option<PathBuf> {
        let user_config = self.manifest.get("UserConfig").and_then(single_obj)?;
        user_config.get("platform_override_dest").and_then(single_str)
            .filter(|v| *v == "linux")?;
        let app_id = self.manifest.get("appid").and_then(single_str)?;
        Some(
            #[rustfmt::skip]
            self.library_path.join(
                ["steamapps", "compatdata", app_id, "pfx", "drive_c"]
                .iter().collect::<PathBuf>(),
            ),
        )
    }

    fn install_dir(&self) -> Option<PathBuf> {
        #[rustfmt::skip]
        let value = Some(self.library_path.join("steamapps").join("common")
            .join(&self.manifest.get("installdir").and_then(single).and_then(Value::get_str)?));
        value
    }

    fn user_dir(&self) -> PathBuf {
        self.compat_drive().map_or_else(
            || dirs::home_dir().expect("dirs::home_dir"),
            |compat_drive| compat_drive.join("users").join("steamuser"),
        )
    }
}

fn find_steam_app(app_id: &str) -> Result<OwningHandle<String, Box<SteamApp>>> {
    let library_dirs = get_steam_library_dirs()?;
    let manifest_file_name: OsString = format!("appmanifest_{}.acf", app_id).into();
    for library_dir in library_dirs {
        for dir_entry in library_dir.join("steamapps").read_dir()? {
            let dir_entry = dir_entry?;
            if dir_entry.file_name() == manifest_file_name {
                let file_string = fs::read_to_string(dir_entry.path())?;
                return OwningHandle::try_new(file_string, |file_str| {
                    // Safety: I have no idea.
                    let parsed = Vdf::parse(unsafe { file_str.as_ref().unwrap() })?;
                    let manifest = match parsed.value {
                        Value::Obj(obj) => obj,
                        _ => bail!("should be obj")
                    };
                    Ok(Box::new(SteamApp {
                        library_path: library_dir,
                        manifest})) 
                    }
                )
            }
        }
    }
    Err(Error::SteamAppNotFound(app_id.to_owned()).into())
}

#[derive(Debug, Clone, Default)]
pub struct Directories {
    mods: PathBuf,
    progress: PathBuf,
    main: PathBuf,
}

fn find_teardown_dirs() -> Result<Directories> {
    let steam_app = find_steam_app("1167630")?;
    let user_dir = steam_app.user_dir();
    // return Err(Error::UnexpectedVDF.into());
    Ok(Directories {
        mods: user_dir.join("My Documents").join("Teardown").join("mods"),
        progress: user_dir
            .join("Local Settings")
            .join("Application Data")
            .join("Teardown"),
        main: steam_app.install_dir().ok_or(Error::NoSteamInstallDir)?,
    })
}
