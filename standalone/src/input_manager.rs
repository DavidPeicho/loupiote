use crate::commands::EditorCommand;
use winit::{
    event::ElementState,
    keyboard::{Key, NamedKey},
};

#[derive(Default)]
pub struct InputManager {}

impl InputManager {
    pub fn new() -> Self {
        Self {}
    }

    pub fn process_keyboard_input(
        &self,
        keycode: &Key,
        state: &ElementState,
    ) -> Option<EditorCommand> {
        // @todo: Mapping should be performed using a config file.
        match (keycode, state) {
            (Key::Named(NamedKey::Space), ElementState::Pressed) => {
                Some(EditorCommand::ToggleAccumulation)
            }
            _ => None,
        }
    }
}
