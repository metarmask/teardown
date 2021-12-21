#![allow(clippy::default_trait_access)] // Default UI state is irrelevant
mod alphanum_ord;
mod style;

use std::{
    fmt::{self, Debug, Formatter},
    fs::{self, ReadDir}, mem,
    path::{PathBuf, Path},
    sync::{Arc, Mutex}, backtrace::BacktraceStatus,
};

use anyhow::{Result, Context};
use iced::{
    button, executor, scrollable, Align, Application, Button, Clipboard, Column, Command, Element,
    Length, Row, Rule, Scrollable, Space, Text, VerticalAlignment, TextInput, text_input,
};
use lazy_static::lazy_static;
use regex::Regex;
use teardown_bin_format::{parse_file, OwnedScene};
use teardown_editor_format::{util::UnwrapLock, vox, SceneWriterBuilder};

use self::alphanum_ord::AlphanumericOrd;
use crate::{find_teardown_dirs, Directories, Error, load_level_meta, GameLuaMeta};

pub struct MainView {
    dirs: Directories,
    levels: Vec<Level>,
    n_special_levels: usize,
    selected_level: Option<usize>,
    vox_store: Arc<Mutex<vox::Store>>,
    button_help: button::State,
    scroll_state: scrollable::State,
}

enum Load<T> {
    None,
    Loading,
    Loaded(T),
}

struct Level {
    path: PathBuf,
    name: String,
    scene: Load<OwnedScene>,
    button_select: button::State,
    button_to_xml: button::State,
    button_to_blender: button::State
}

fn level_path_to_id(path: &Path) -> String {
    let mut path = path.to_path_buf();
    path.set_extension("");
    let file_name = path.file_name().unwrap_or_default();
    file_name.to_string_lossy().to_string()
}

struct LevelViews<'a> {
    button: Button<'a, MainMessage>,
    side: Option<Element<'a, LevelMessage>>,
}

fn write_scene_and_vox(
    scene_writer_builder: &SceneWriterBuilder,
    vox_store: &Arc<Mutex<vox::Store>>,
) -> Result<()> {
    scene_writer_builder
        .build()
        .map_err(Error::SceneWriterBuild)?
        .write_scene()?;
    vox_store.unwrap_lock().write_dirty()?;
    Ok(())
}

impl Level {
    fn new(path: PathBuf, name: String) -> Self {
        Self {
            path, name, scene: Load::None,
            button_select: Default::default(),
            button_to_xml: Default::default(),
            button_to_blender: Default::default(),
        }
    }

    #[rustfmt::skip]
    fn view(&mut self, selected: bool) -> LevelViews {
        let side = if selected {
            Some(match &self.scene {
                Load::None => { Text::new("").into() }
                Load::Loading => {
                    Column::with_children(vec![
                        Text::new("Loading ...").into()
                    ]).padding(5).into()
                }
                Load::Loaded(scene) => {
                    Column::with_children(vec![
                        Column::with_children(vec![
                            Text::new(scene.level).into(),
                            Text::new(format!("Entities: {}", scene.iter_entities().count())).into(),
                        ]).padding(5).into(),
                        Space::with_height(Length::Fill).into(),
                        Row::with_children(vec![
                            Text::new("Convert to ...".to_string())
                                .vertical_alignment(VerticalAlignment::Center).into(),
                            Space::with_width(10.into()).into(),
                            Button::new(&mut self.button_to_xml, Text::new("Editor"))
                                .width(Length::Fill)
                                .on_press(LevelMessage::ConvertXML).into(),
                            // Space::with_width(Length::Fill).into(),
                            Button::new(&mut self.button_to_blender, Text::new("Blender"))
                            .width(Length::Fill).into()
                        ]).align_items(Align::Center).padding(15).into()
                    ]).into()
                }
            })
        } else { None };
        let button = {
            let text = Text::new(self.name.clone());
            let mut button = Button::new(&mut self.button_select, Row::with_children(vec![text.into(), Space::with_width(Length::Fill).into()]));
            button = button.style(style::LevelButton {
                selected: selected || matches!(self.scene, Load::Loading),
                loaded: matches!(self.scene, Load::Loaded(_)) });
            button
        };
        LevelViews { button, side }
    }

    fn update(
        &mut self,
        dirs: &Directories,
        vox_store: &Arc<Mutex<vox::Store>>,
        message: LevelMessage,
    ) -> Command<LevelMessage> {
        match message {
            LevelMessage::ConvertXML => match mem::replace(&mut self.scene, Load::Loading) {
                Load::Loaded(scene) => {
                    let dirs = dirs.clone();
                    let vox_store = vox_store.clone();
                    return Command::perform(
                        async move {
                            let mut builder = SceneWriterBuilder::default();
                            builder
                                .vox_store(vox_store.clone())
                                .mod_dir(dirs.mods.join("converted"))
                                .scene(&scene);
                            write_scene_and_vox(&builder, &vox_store).map(|_| scene)
                        },
                        |scene_result| match scene_result {
                            Ok(scene) => LevelMessage::XMLConverted(Arc::new(scene)),
                            Err(err) => LevelMessage::Error(Arc::new(err)),
                        },
                    );
                }
                other => self.scene = other,
            },
            LevelMessage::SceneLoaded(scene) => {
                self.scene = Load::Loaded(if let Ok(scene_result) = Arc::try_unwrap(scene) {
                    match scene_result {
                        Ok(scene) => scene,
                        Err(error) => {
                            // Let this be caught by Main
                            return Command::perform(
                                async move { LevelMessage::Error(Arc::new(anyhow::Error::msg(error))) },
                                |level_message| level_message,
                            )
                        }
                    }
                } else {
                    panic!("Arc::try_unwrap")
                });
            }
            LevelMessage::XMLConverted(scene) => {
                self.scene = Load::Loaded(if let Ok(ok) = Arc::try_unwrap(scene) {
                    ok
                } else {
                    panic!("Arc::try_unwrap")
                });
            }
            LevelMessage::Error(error) => {
                panic!("{:?}", error);
            }
        }

        Command::none()
    }

    fn load_scene(&mut self, force: bool) -> Command<LevelMessage> {
        let no_scene = matches!(self.scene, Load::None);

        if no_scene || force {
            let path = self.path.clone();
            self.scene = Load::Loading;
            Command::perform(async { parse_file(path) }, |w| {
                LevelMessage::SceneLoaded(Arc::new(w))
            })
        } else {
            Command::none()
        }
    }
}

#[derive(Clone)]
pub enum LevelMessage {
    ConvertXML,
    SceneLoaded(Arc<Result<OwnedScene>>),
    XMLConverted(Arc<OwnedScene>),
    Error(Arc<anyhow::Error>),
}

#[derive(Clone)]
pub enum MainMessage {
    Level(usize, LevelMessage),
    SelectLevel(usize),
    Help,
    HelpQuit,
    Error(Arc<anyhow::Error>),
}

impl Debug for MainMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("hi")
    }
}

fn name_from_level_id(id_name: &str, lua_game_meta: &GameLuaMeta) -> Option<String> {
    if let Some(props) = lua_game_meta.sandbox.get(id_name) {
        return Some(format!("Sandbox: {}", props.get("name")?))
    }
    if let Some(mission) = lua_game_meta.missions.get(id_name) {
        let level_name = mission.get("level")?;
        let level = lua_game_meta.levels.get(level_name)?;
        return Some(format!("Mission: {} - {}", level.get("title")?, mission.get("title")?))
    }
    if let Some(challenge) = lua_game_meta.challenges.get(id_name) {
        let level = lua_game_meta.levels.get(challenge.get("level")?)?;
        return Some(format!("Challenge: {} - {}", level.get("title")?, challenge.get("title")?))
    }
    for (cinematic_name, parts) in lua_game_meta.cinematic_parts.iter() {
        for (part_n, part) in parts.iter().enumerate() {
            if part.get("id")? == id_name {
                let file = part.get("file")?;
                // let layers = part.get("layers")?;
                return Some(format!("Cinematic part: {}:{} on {}", cinematic_name, part_n, file))
            }
        }
    }
    None
}

fn read_dir_with_ctx<P: AsRef<Path>>(path: P) -> Result<ReadDir> {
    fs::read_dir(path.as_ref()).context(format!("Reading directory \"{}\"", path.as_ref().display()))
}

fn init_levels(dirs: &Directories, lua_game_meta: GameLuaMeta) -> Result<Vec<Level>> {
    let mut levels = read_dir_with_ctx(dirs.main.join("data").join("bin"))?
        .map(|res| res.map(|dir_entry| {
            let path = dir_entry.path();
            let id = level_path_to_id(&path);
            Level::new(path, name_from_level_id(&id, &lua_game_meta).unwrap_or(id))
        }))
        .collect::<Result<Vec<_>, _>>()?;
    levels.sort_by_cached_key(|x| AlphanumericOrd(x.name.clone()));
    levels.insert(0, Level::new(dirs.progress.join("quicksave.bin"), "Last quicksave".to_string()));
    Ok(levels)
}

impl MainView {
    fn new(dirs: Directories) -> Result<Self> {
        let levels = init_levels(&dirs, load_level_meta()?)?;
        let vox_store = vox::Store::new(&dirs.main)?;
        Ok(MainView {
            levels,
            n_special_levels: 1,
            selected_level: None,
            vox_store,
            dirs,
            button_help: Default::default(),
            scroll_state: Default::default(),
        })
    }

    fn update(&mut self, message: MainMessage) -> Command<MainMessage> {
        match message {
            MainMessage::Level(level, message) => {
                // Let this error be caught by App
                if let LevelMessage::Error(error) = message {
                    return Command::perform(
                        async move { MainMessage::Error(error) },
                        |result| result,
                    )
                }
                self
                .levels
                .get_mut(level)
                .expect("no level")
                .update(&self.dirs, &self.vox_store, message)
                .map(move |what| MainMessage::Level(level, what))
            },
            MainMessage::SelectLevel(level) => {
                let already_selected = self.selected_level == Some(level);
                self.selected_level = Some(level);
                self.levels
                    .get_mut(level)
                    .unwrap()
                    .load_scene(already_selected)
                    .map(move |what| MainMessage::Level(level, what))
            }
            // Message::LoadError(err) => {
            //     eprintln!("Load error: {:?}", err);
            //     Command::none()
            // }
            MainMessage::Help => Command::perform(
                async move { open::that("https://github.com/metarmask/teardown") },
                |result| {
                    if let Err(err) = result {
                        MainMessage::Error(Arc::new(err.into()))
                    } else {
                        MainMessage::HelpQuit
                    }
                },
            ),
            MainMessage::HelpQuit => Command::none(),
            MainMessage::Error(_) => unreachable!("caught by App"),
        }
    }

    #[rustfmt::skip]
    fn view(&mut self) -> Element<'_, MainMessage> {
        let selected_level = self.selected_level;
        let (level_buttons, mut level_side_views) = self.levels.iter_mut().enumerate().map(|(i, level)| {
            let mut view = level.view(selected_level == Some(i));
            view.button = view.button.on_press(MainMessage::SelectLevel(i));
            (view.button, view.side)
        }).unzip::<_, _, Vec<_>, Vec<_>>();
        Column::with_children(vec![
            Row::with_children(vec![
                Text::new(format!("{} palette files cached", self.vox_store.unwrap_lock().palette_files.len())).into(),
                Space::with_width(Length::Fill).into(),
                Button::new(&mut self.button_help, Text::new("Help"))
                .on_press(MainMessage::Help)
                .into()
            ])
            .padding(20)
            .into(),
            Row::with_children(vec![
                Column::with_children({
                    let mut level_buttons_iter = level_buttons.into_iter();
                    let special_buttons = level_buttons_iter.by_ref().take(self.n_special_levels).map(Into::into).collect::<Vec<_>>();
                    let mut scrollable = Scrollable::new(&mut self.scroll_state).style(style::Theme);
                    for button in level_buttons_iter {
                        scrollable = scrollable.push(button);
                    }
                    vec![
                        Column::with_children(special_buttons).into(),
                        Rule::horizontal(2).into(),
                        scrollable.into()
                    ]
                })
                .width(Length::FillPortion(1)).into(),
                Column::with_children(if let Some(selected) = self.selected_level {
                    if let Some(level_side_view) = level_side_views.remove(selected) {
                        vec![level_side_view.map(move |level_message| MainMessage::Level(selected, level_message))]
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                })
                .width(Length::FillPortion(2)).into(),
            ]).into()
        ])
        .into()
    }
}

pub struct SetDirectoriesView {
    error: Option<ErrorView>,
    inputs: Vec<FormInput>,
    cont: button::State
}

#[derive(Debug, Clone)]
pub struct FormInput {
    name: String,
    value: String,
    text_input: text_input::State,
}

#[derive(Debug, Clone)]
pub struct NewValueMessage {
    name: String,
    value: String
}

impl FormInput {
    fn new<A: ToOwned<Owned = String> + ?Sized, B: ToOwned<Owned = String> + ?Sized>(name: &A, value: &B) -> FormInput {
        Self {
            name: name.to_owned(), value: value.to_owned(), text_input: Default::default()
        }
    }

    fn view(&mut self) -> Element<'_, NewValueMessage> {
        let name = self.name.clone();
        Element::new(TextInput::new(&mut self.text_input, &self.name, &self.value, move |value| {
            NewValueMessage { name: name.clone(), value }
        }))
    }
}

pub enum App {
    Main(MainView),
    SetDirectories(SetDirectoriesView),
    Error(ErrorView),
}

pub struct ErrorView {
    error: Arc<anyhow::Error>,
    scroll_state: scrollable::State,
}

impl ErrorView {
    fn new(error: anyhow::Error) -> Self {
        Self { error: Arc::new(error), scroll_state: Default::default() }
    }

    fn view<'a, T: 'a>(&'a mut self) -> Scrollable<'a, T> {
        let formatted_error = format_error(&self.error);
        eprintln!("{}", formatted_error);
        Scrollable::new(&mut self.scroll_state).style(style::Theme)
        .push(Text::new(formatted_error))
    }
}

#[derive(Debug, Clone)]
pub enum AppMessage {
    Main(MainMessage),
    SetDirectoriesContinue,
    FormInput(NewValueMessage)
}

fn format_error(error: &anyhow::Error) -> String {
    lazy_static! {
        static ref RE_AT: Regex = Regex::new(r"\s*(\d+): (.+)\n(?:\s+at (.+))?").unwrap();
        static ref RE_PATH_PREFIX: Regex = Regex::new(r"(?:^/.*?/\.?cargo/registry/src/.+?/|^/rustc/.+?/)").unwrap();
    }
    let mut s = format!("{}", error);
    if let Some(cause) = error.source() {
        s += "\n\nCaused by:";
        for (n, cause) in anyhow::Chain::new(cause).enumerate() {
            s += "\n";
            s.extend(format!("{}: {}\n", n, cause).lines().map(|line| format!("    {}", line)).intersperse("\n".to_string()));
        }
    }
    let backtrace = error.backtrace();
    if let BacktraceStatus::Captured = backtrace.status() {
        s += "\n\nBacktrace:\n";
        for re_match in RE_AT.captures_iter(&backtrace.to_string()) {
            let n = re_match.get(1).map(|m| m.as_str()).unwrap();
            let symbol = re_match.get(2).map(|m| m.as_str()).unwrap();
            if symbol == "std::sys_common::backtrace::__rust_begin_short_backtrace" { break }
            s += &format!("{}: {}\n", n, symbol);
            if let Some(path) = re_match.get(3) {
                let path_str = path.as_str().to_string();
                s += &format!("    at {}\n", RE_PATH_PREFIX.replace_all(&path_str, ""))
            }
        }
    }
    s
}

fn dirs_from_input(inputs: Vec<FormInput>) -> Result<Directories> {
    Ok(Directories {
        mods: PathBuf::from(inputs.iter().find(|i| i.name == "Mods").unwrap().value.clone()),
        progress: PathBuf::from(inputs.iter().find(|i| i.name == "Progress").unwrap().value.clone()),
        main: PathBuf::from(inputs.iter().find(|i| i.name == "Main").unwrap().value.clone()),
    })
}

impl Application for App {
    type Message = AppMessage;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Self::Message>) {
        (
            match find_teardown_dirs() {
                Ok(dirs) => {
                    match MainView::new(dirs) {
                        Ok(main) => App::Main(main),
                        Err(err) => App::Error(ErrorView { error: Arc::new(err), scroll_state: Default::default()}),
                    }
                },
                Err(err) => App::SetDirectories(SetDirectoriesView { error: Some(ErrorView::new(err)), cont: Default::default(), inputs: vec![
                    FormInput::new("Mods", ""),
                    FormInput::new("Progress", ""),
                    FormInput::new("Main", "")
                ] })
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        "Parse and convert the binary format for Teardown".to_string()
    }

    fn update(
        &mut self,
        message: Self::Message,
        _clipboard: &mut Clipboard,
    ) -> Command<Self::Message> {
        match message {
            AppMessage::Main(main_message) => match self {
                App::Main(main_view) => match main_message {
                    MainMessage::Error(error) => {
                        *self = App::Error(ErrorView { error, scroll_state: Default::default() });
                        Command::none()
                    }
                    other => main_view.update(other).map(AppMessage::Main),
                },
                App::Error(_) => {
                    eprintln!("MainView could not receive a message because of the current error");
                    Command::none()
                }
                App::SetDirectories(_) => todo!(),
            },
            AppMessage::SetDirectoriesContinue => {
                match self {
                    App::SetDirectories(SetDirectoriesView { inputs, .. }) => {
                        *self = match MainView::new(dirs_from_input(inputs.to_vec()).unwrap()) {
                            Ok(main) => App::Main(main),
                            Err(err) => App::Error(ErrorView { error: Arc::new(err), scroll_state: Default::default()}),
                        }
                    },
                    _ => unreachable!("continue set directories")
                }
                
                Command::none()
            },
            AppMessage::FormInput(new_value) => {
                match self {
                    App::SetDirectories(a) => {
                        for input in a.inputs.iter_mut() {
                            if input.name == new_value.name {
                                input.value = new_value.value;
                                break
                            }
                        }
                        Command::none()
                    },
                    _ => unreachable!("no form input in the other views")
                }
            },
        }
    }

    fn view(&mut self) -> Element<'_, Self::Message> {
        match self {
            App::Main(main_view) => main_view.view().map(AppMessage::Main),
            App::SetDirectories(SetDirectoriesView { error, cont, inputs }) => {
                let mut column_children = vec![];
                if let Some(err) = error {
                    column_children.push(Element::new(err.view().max_height(95)));
                }
                column_children.append(&mut vec![
                    Element::new(Column::with_children(inputs.iter_mut().map(|input| {
                        input.view().map(AppMessage::FormInput)
                    }).collect::<Vec<_>>())),
                    Element::new(Button::new(cont, Text::new("Continue"))
                    .on_press(AppMessage::SetDirectoriesContinue))
                ]);
                Element::new(Column::with_children(column_children))
            },
            App::Error(error_view) => error_view.view().into(),
        }
    }
}
