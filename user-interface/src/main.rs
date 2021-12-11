#![feature(stmt_expr_attributes)]
#[cfg(feature = "graphical")]
mod graphical;

use std::{
    collections::HashSet,
    ffi::OsString,
    fs, iter,
    path::{Path, PathBuf},
};

use ::vox as vox_format;
use anyhow::{Context, Result};
use clap::ArgGroup;
#[cfg(feature = "graphical")]
use iced::{Application, Settings};
use steamy_vdf as vdf;
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
                        .find(|(other_name, _)| name == other_name.as_ref()).context("No level with that name")?
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
        ["steamapps", "libraryfolders.vdf"]
            .iter()
            .collect::<PathBuf>(),
    );
    let vdf = steamy_vdf::load(&library_dirs_path)?;
    #[rustfmt::skip]
    let entries = vdf
        .as_table().context(VDFErr)?
        .get("LibraryFolders").context(VDFErr)?
        .as_table().context(VDFErr)?;
    let mut libraries = Vec::new();
    for (key, value) in entries.iter() {
        if key.parse::<i32>().is_ok() {
            libraries.push(value.as_str().context(VDFErr)?.into());
        }
    }
    Ok(libraries)
}

fn get_steam_library_dirs() -> Result<Vec<PathBuf>> {
    let main_dir = get_steam_dir()?;
    let extras = get_extra_library_dirs(&main_dir)
        .context("Could not get extra library folders")
        .unwrap_or_else(|err| {
            eprintln!("{:#}", err);
            Vec::new()
        });
    Ok(iter::once(main_dir).chain(extras).collect())
}

struct SteamApp {
    manifest: vdf::Table,
    library_path: PathBuf,
}

impl SteamApp {
    fn compat_drive(&self) -> Option<PathBuf> {
        if let Some(user_config) = self.manifest.get("UserConfig") {
            if let Some(override_dest) = user_config.as_table()?.get("platform_override_dest") {
                if override_dest.as_str()? == "linux" {
                    return Some(
                        #[rustfmt::skip]
                        self.library_path.join(
                            ["steamapps", "compatdata", self.manifest.get("appid")?.as_str()?, "pfx", "drive_c"]
                            .iter().collect::<PathBuf>(),
                        ),
                    );
                }
            }
        }
        None
    }

    fn install_dir(&self) -> Option<PathBuf> {
        #[rustfmt::skip]
        Some(self.library_path.join("steamapps").join("common")
            .join(&self.manifest.get("installdir")?.as_str()?))
    }

    fn user_dir(&self) -> PathBuf {
        self.compat_drive().map_or_else(
            || dirs::home_dir().expect("dirs::home_dir"),
            |compat_drive| compat_drive.join("users").join("steamuser"),
        )
    }
}

fn find_steam_app(app_id: &str) -> Result<SteamApp> {
    let library_dirs = get_steam_library_dirs()?;
    let manifest_file_name: OsString = format!("appmanifest_{}.acf", app_id).into();
    for library_dir in library_dirs {
        for dir_entry in library_dir.join("steamapps").read_dir()? {
            let dir_entry = dir_entry?;
            if dir_entry.file_name() == manifest_file_name {
                #[rustfmt::skip]
                let manifest =
                    steamy_vdf::load(dir_entry.path())?
                    .as_table().context(VDFErr)?
                    .get("AppState").context(VDFErr)?
                    .as_table().context(VDFErr)?
                    .clone();
                return Ok(SteamApp {
                    library_path: library_dir,
                    manifest,
                });
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
    Ok(Directories {
        mods: user_dir.join("My Documents").join("Teardown").join("mods"),
        progress: user_dir
            .join("Local Settings")
            .join("Application Data")
            .join("Teardown"),
        main: steam_app.install_dir().ok_or(Error::NoSteamInstallDir)?,
    })
}
