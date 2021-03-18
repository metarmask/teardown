mod style;

use std::{fs, path::PathBuf};
use iced::{Align, Button, Column, Element, Length, Row, Rule, Sandbox, Scrollable, Space, Text, VerticalAlignment, button, scrollable};
use teardown_bin_format::{Scene, parse_file};
use owning_ref::OwningHandle;
use teardown_editor_format::VoxStore;
use crate::{Directories, find_teardown_dirs};

pub struct App<'a> {
    dirs: Directories,
    special_file_buttons: Vec<LevelButton>,
    file_buttons: Vec<LevelButton>,
    loaded_scene: Option<OwningHandle<Vec<u8>, std::boxed::Box<Scene<'a>>>>,
    vox_store: Box<VoxStore>,
    button_to_xml: button::State,
    button_to_blender: button::State,
    scroll_state: scrollable::State
}

#[derive(Default)]
struct LevelButton {
    selected: bool,
    path: Option<PathBuf>,
    button_state: button::State
}

impl LevelButton {
    fn name(&self) -> String {
        if let Some(mut path) = self.path.to_owned() {
            path.set_extension("");
            path.file_name().unwrap().to_string_lossy().to_string()
        } else {
            "custom".to_owned()
        }
    }

    fn view(&mut self) -> Element<'_, <App as Sandbox>::Message> {
        let text = Text::new(self.name());
        let mut button = Button::new(&mut self.button_state, Row::with_children(vec![text.into(), Space::with_width(Length::Fill).into()]))
            .on_press(Message::LevelButtonPressed(self.path.to_owned().unwrap_or_default()));
        button = if self.selected {
            button.style(style::SelectedListButton)
        } else {
            button.style(style::ListButton)
        };
        button.into()
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    ConvertXML,
    LevelButtonPressed(PathBuf),
    LoadError(String)
}

impl Sandbox for App<'_> {
    type Message = Message;

    fn new() -> Self {
        let dirs = find_teardown_dirs().unwrap();
        let mut file_buttons = fs::read_dir(dirs.main.join("data").join("bin")).unwrap()
            .map(|res| res.map(|dir_entry| {
                let button = LevelButton {
                    path: Some(dir_entry.path()),
                    .. Default::default()
                };
                let name = button.name();
                (name, button)
            }))
            .collect::<Result<Vec<_>, _>>().unwrap();
        file_buttons.sort_by(|a, b| alphanumeric_sort::compare_str(&a.0, &b.0));
        let file_buttons = file_buttons.into_iter().map(|(_, button)| button).collect();
        App {
            vox_store: Box::new(VoxStore {
                hashed_vox_dir: Some(dirs.main.join("data").join("vox").join("hash")),
                dirty: Default::default(),
                files: Default::default()
            }),
            special_file_buttons: vec![
                LevelButton {
                    path: Some(dirs.progress.join("quicksave.bin")),
                    .. Default::default()
                },
                // LevelButton::default()
            ],
            dirs,
            file_buttons,
            button_to_blender: Default::default(),
            button_to_xml: Default::default(),
            loaded_scene: Default::default(),
            scroll_state: Default::default()
        }
    }

    fn title(&self) -> String {
        format!("Parse and convert the binary format for Teardown")
    }

    fn update(&mut self, message: Self::Message) {
        match message {
            Message::ConvertXML => {
                let dirs = &self.dirs;
                teardown_editor_format::write_scene(self.loaded_scene.as_ref().unwrap(), &dirs.main, dirs.mods.join("converted"), "main", &mut self.vox_store).unwrap();
                (&mut self.vox_store).write_dirty().unwrap();
            }
            Message::LevelButtonPressed(path) => {
                for button in self.special_file_buttons.iter_mut().chain(self.file_buttons.iter_mut()) {
                    button.selected = if let Some(other_path) = &button.path {
                        path == *other_path
                    } else { false };
                }
                self.loaded_scene = Some(parse_file(path).expect("Parsing the level..."));
            }
            Message::LoadError(_) => {}
        }
    }

    fn view(&mut self) -> Element<'_, Self::Message> {
        // Column::with_children(vec![
        //     Row::with_children(vec![
        //         Space::with_width(Length::Fill).into(),
        //         Button::new(&mut self.button_settings, Text::new("Settings"))
        //         .into()
        //     ])
        //     .padding(20)
        //     .into(),
            Row::with_children(vec![
                Column::with_children(vec![
                    {
                        let mut column = Column::new();
                        for button in self.special_file_buttons.iter_mut() {
                            column = column.push(button.view());
                        }
                        column
                    }.into(),
                    Rule::horizontal(2).into(),
                    {
                        let mut scrollable = Scrollable::new(&mut self.scroll_state);
                        for button in self.file_buttons.iter_mut() {
                            scrollable = scrollable.push(button.view());
                        }
                        scrollable
                    }.into()
                ])
                .width(Length::FillPortion(1)).into(),
                Column::with_children(vec![
                    if let Some(scene) = &self.loaded_scene {
                        Element::from(Column::with_children(vec![
                            Column::with_children(vec![
                                Text::new(scene.level).into(),
                                Text::new(format!("Entities: {}", scene.iter_entities().count())).into(),
                            ]).into(),
                            Space::with_height(Length::Fill).into(),
                            Row::with_children(vec![
                                Text::new(format!("Convert to ..."))
                                    .vertical_alignment(VerticalAlignment::Center).into(),
                                Space::with_width(10.into()).into(),
                                Button::new(&mut self.button_to_xml, Text::new("Editor"))
                                    .width(Length::Fill)
                                    .on_press(Message::ConvertXML).into(),
                                // Space::with_width(Length::Fill).into(),
                                Button::new(&mut self.button_to_blender, Text::new("Blender"))
                                .width(Length::Fill).into()
                            ]).align_items(Align::Center).padding(15).into()
                        ]))
                    } else {
                        Element::from(Space::with_height(0.into()))
                    }
                ])
                .width(Length::FillPortion(2)).into(),
            ]).into()
        // ])
        // .into()
    }
}
