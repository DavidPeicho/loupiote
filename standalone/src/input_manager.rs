use crate::commands::EditorCommand;
use winit::event::{ElementState, VirtualKeyCode};

pub struct InputManager {}

impl InputManager {
    pub fn new() -> Self {
        Self {}
    }

    pub fn process_keyboard_input(
        &self,
        keycode: &VirtualKeyCode,
        state: &ElementState,
    ) -> Option<EditorCommand> {
        // @todo: Mapping should be performed using a config file.
        match (keycode, state) {
            (VirtualKeyCode::Space, ElementState::Pressed) => {
                Some(EditorCommand::ToggleAccumulation)
            }
            _ => None,
        }
    }
}
