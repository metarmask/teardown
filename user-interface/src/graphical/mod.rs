#![allow(clippy::default_trait_access)] // Default UI state is irrelevant
mod alphanum_ord;
mod style;

use std::{
    fmt::{self, Debug, Formatter},
    fs, mem,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use iced::{
    button, executor, scrollable, Align, Application, Button, Column, Command, Element, Length,
    Row, Rule, Scrollable, Space, Text, VerticalAlignment,
};
use owning_ref::OwningHandle;
use teardown_bin_format::{parse_file, OwnedScene, Scene};
use teardown_editor_format::{SceneWriterBuilder, VoxStore};

use self::alphanum_ord::AlphanumericOrd;
use crate::{find_teardown_dirs, Directories};

pub struct App {
    dirs: Directories,
    levels: Vec<Level>,
    n_special_levels: usize,
    selected_level: Option<usize>,
    vox_store: Arc<Mutex<VoxStore>>,
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
    button: Button<'a, <App as Application>::Message>,
    side: Option<Element<'a, LevelMessage>>,
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
        path.file_name().unwrap().to_string_lossy().to_string()
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
        vox_store: &Arc<Mutex<VoxStore>>,
        message: LevelMessage,
    ) -> Command<LevelMessage> {
        match message {
            LevelMessage::ConvertXML => match mem::replace(&mut self.scene, Load::Loading) {
                Load::Loaded(scene) => {
                    let dirs = dirs.clone();
                    let vox_store = vox_store.clone();
                    return Command::perform(
                        async move {
                            let dirs = dirs;
                            let scene = scene;
                            SceneWriterBuilder::default()
                                .vox_store(vox_store.clone())
                                .mod_dir(dirs.mods.join("converted"))
                                .scene(&scene)
                                .build()
                                .unwrap()
                                .write_scene()
                                .unwrap();
                            vox_store.lock().unwrap().write_dirty().unwrap();
                            scene
                        },
                        |scene| LevelMessage::XMLConverted(Arc::new(scene)),
                    );
                }
                other => self.scene = other,
            },
            LevelMessage::SceneLoaded(scene) => {
                self.scene = Load::Loaded(if let Ok(ok) = Arc::try_unwrap(scene) {
                    ok.expect("error loading scene")
                } else {
                    panic!("Arc::try_unwrap")
                })
            }
            LevelMessage::XMLConverted(scene) => {
                self.scene = Load::Loaded(if let Ok(ok) = Arc::try_unwrap(scene) {
                    ok
                } else {
                    panic!("Arc::try_unwrap")
                })
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
                LevelMessage::SceneLoaded(Arc::new(w.map_err(|err| err.to_string())))
            })
        } else {
            Command::none()
        }
    }
}

#[derive(Clone)]
pub enum LevelMessage {
    ConvertXML,
    SceneLoaded(Arc<Result<OwnedScene, String>>),
    XMLConverted(Arc<OwnedScene>),
}

#[derive(Clone)]
pub enum Message {
    Level(usize, LevelMessage),
    SelectLevel(usize),
    // LoadError(String),
    Help,
    HelpQuit,
}

impl Debug for Message {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("hi")
    }
}

impl Application for App {
    type Message = Message;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Self::Message>) {
        let dirs = find_teardown_dirs().unwrap();
        let mut levels = fs::read_dir(dirs.main.join("data").join("bin"))
            .unwrap()
            .map(|res| res.map(|dir_entry| Level::new(dir_entry.path())))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        levels.sort_by_cached_key(|x| AlphanumericOrd(x.name()));
        levels.insert(0, Level::new(dirs.progress.join("quicksave.bin")));
        (
            App {
                // levels: levels.into_iter().map(|level| ByAddress(Arc::new(level))).collect(),
                levels,
                n_special_levels: 1,
                selected_level: None,
                vox_store: VoxStore::new(&dirs.main).unwrap(),
                dirs,
                button_help: Default::default(),
                scroll_state: Default::default(),
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        "Parse and convert the binary format for Teardown".to_string()
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            Message::Level(level, message) => self
                .levels
                .get_mut(level)
                .expect("no level")
                .update(&self.dirs, &self.vox_store, message)
                .map(move |what| Message::Level(level, what)),
            Message::SelectLevel(level) => {
                let already_selected = self.selected_level == Some(level);
                self.selected_level = Some(level);
                self.levels
                    .get_mut(level)
                    .unwrap()
                    .load_scene(already_selected)
                    .map(move |what| Message::Level(level, what))
            }
            // Message::LoadError(err) => {
            //     eprintln!("Load error: {:?}", err);
            //     Command::none()
            // }
            Message::Help => Command::perform(
                async move { open::that("https://github.com/metarmask/teardown").unwrap() },
                |_| Message::HelpQuit,
            ),
            Message::HelpQuit => Command::none(),
        }
    }

    #[rustfmt::skip]
    fn view(&mut self) -> Element<'_, Self::Message> {
        let selected_level = self.selected_level;
        let (level_buttons, mut level_side_views) = self.levels.iter_mut().enumerate().map(|(i, level)| {
            let mut view = level.view(selected_level == Some(i));
            view.button = view.button.on_press(Message::SelectLevel(i));
            (view.button, view.side)
        }).unzip::<_, _, Vec<_>, Vec<_>>();
        Column::with_children(vec![
            Row::with_children(vec![
                Text::new(format!("{} palette files cached", self.vox_store.lock().unwrap().palette_files.len())).into(),
                Space::with_width(Length::Fill).into(),
                Button::new(&mut self.button_help, Text::new("Help"))
                .on_press(Message::Help)
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
                    vec![
                        level_side_views.remove(selected).unwrap()
                        .map(move |level_message| Message::Level(selected, level_message))]
                } else {
                    vec![]
                })
                .width(Length::FillPortion(2)).into(),
            ]).into()
        ])
        .into()
    }
}
