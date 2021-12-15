#![allow(clippy::default_trait_access)] // Default UI state is irrelevant
mod alphanum_ord;
mod style;

use std::{
    fmt::{self, Debug, Formatter},
    fs, mem,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::Result;
use iced::{
    button, executor, scrollable, Align, Application, Button, Clipboard, Column, Command, Element,
    Length, Row, Rule, Scrollable, Space, Text, VerticalAlignment, TextInput, text_input
};
use owning_ref::OwningHandle;
use teardown_bin_format::{parse_file, OwnedScene, Scene};
use teardown_editor_format::{util::UnwrapLock, vox, SceneWriterBuilder};

use self::alphanum_ord::AlphanumericOrd;
use crate::{find_teardown_dirs, Directories, Error};

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
    scene: Load<OwningHandle<Vec<u8>, std::boxed::Box<Scene<'static>>>>,
    button_select: button::State,
    button_to_xml: button::State,
    button_to_blender: button::State,
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
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            scene: Load::None,
            button_select: Default::default(),
            button_to_xml: Default::default(),
            button_to_blender: Default::default(),
        }
    }

    fn name(&self) -> String {
        let mut path = self.path.clone();
        path.set_extension("");
        let file_name = path.file_name().unwrap_or_default();
        file_name.to_string_lossy().to_string()
    }

    #[rustfmt::skip]
    fn view(&mut self, selected: bool) -> LevelViews {
        let name = self.name();
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
            let text = Text::new(name);
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

fn init_levels(dirs: &Directories) -> Result<Vec<Level>> {
    let mut levels = fs::read_dir(dirs.main.join("data").join("bin"))?
        .map(|res| res.map(|dir_entry| Level::new(dir_entry.path())))
        .collect::<Result<Vec<_>, _>>()?;
    levels.sort_by_cached_key(|x| AlphanumericOrd(x.name()));
    levels.insert(0, Level::new(dirs.progress.join("quicksave.bin")));
    Ok(levels)
}

impl MainView {
    fn new() -> Result<Self> {
        let dirs = find_teardown_dirs()?;
        let levels = init_levels(&dirs)?;
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
    text_input: text_input::State,
    dirs: Directories
}

pub enum App {
    Main(MainView),
    SetDirectories(SetDirectoriesView),
    Error(String),
}

#[derive(Debug, Clone)]
pub enum AppMessage {
    Main(MainMessage),
    SetDirectories(Directories),
}

impl Application for App {
    type Message = AppMessage;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Self::Message>) {
        (
            match MainView::new() {
                Ok(main) => App::Main(main),
                Err(err) => App::Error(format!("{:#}", err)),
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
                        *self = App::Error(format!("{:#}", error));
                        Command::none()
                    }
                    other => main_view.update(other).map(AppMessage::Main),
                },
                App::Error(ref mut error) => {
                    *error += "\nMainView could not receive a message because of the current error";
                    Command::none()
                }
                App::SetDirectories(_) => todo!(),
            },
            AppMessage::SetDirectories(_) => todo!(),
        }
    }

    fn view(&mut self) -> Element<'_, Self::Message> {
        match self {
            App::Main(main_view) => main_view.view().map(AppMessage::Main),
            App::SetDirectories(SetDirectoriesView { text_input , dirs }) => Element::new(Column::with_children(vec![
                Element::new(Row::with_children(vec![
                    Element::new(TextInput::new(text_input, "test", &dirs.main.to_string_lossy(), |_| AppMessage::SetDirectories(Directories::default())))
                ]))
            ])),
            App::Error(error) => Text::new(error.clone()).into(),
        }
    }
}
