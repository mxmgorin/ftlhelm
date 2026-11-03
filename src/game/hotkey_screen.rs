//! The hotkey screen's buttons. ControlButton's constructor leaves its state unset, so each
//! options screen the game builds copies one stale value into every button. A stale waiting
//! state shows every key as PRESS KEY, and the next key press rebinds them all.

use super::mem::Obj;
use super::offsets::*;

/// Returns to idle every hotkey button of `options` that waits for a key without being the
/// selected one; the game itself keeps at most that one waiting.
pub fn release_stray_waits(options: Obj) {
    let controls = options.at(OPTIONS_CONTROLS);
    let selected = (controls.i32(CONTROLS_PAGE), controls.i32(CONTROLS_SELECTED));
    for page in 0..CONTROLS_PAGES {
        let buttons = controls.at(page * VECTOR_SIZE);
        let Some(begin) = buttons.obj(VECTOR_BEGIN) else {
            continue;
        };
        for i in 0..buttons.vec_len(CONTROL_BUTTON_SIZE) {
            let button = begin.at(i * CONTROL_BUTTON_SIZE);
            if button.i32(CONTROL_BUTTON_STATE) == CONTROL_WAITING && selected != (page as i32, i as i32) {
                button.set(CONTROL_BUTTON_STATE, CONTROL_IDLE);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::fake::Arena;

    type Button = [i32; CONTROL_BUTTON_SIZE / size_of::<i32>()];
    const STATE: usize = CONTROL_BUTTON_STATE / size_of::<i32>();

    fn button(state: i32) -> Button {
        let mut b = [0; CONTROL_BUTTON_SIZE / size_of::<i32>()];
        b[STATE] = state;
        b
    }

    /// An options screen whose hotkey pages hold buttons in these states.
    fn options(arena: &mut Arena, pages: &[&[i32]], page: i32, selected: i32) -> Obj {
        let options = arena.alloc(OPTIONS_CONTROLS + CONTROLS_PAGE + size_of::<i32>());
        let controls = options.at(OPTIONS_CONTROLS);
        for (p, states) in pages.iter().enumerate() {
            let buttons: Vec<Button> = states.iter().map(|&s| button(s)).collect();
            arena.vector(controls, p * VECTOR_SIZE, &buttons);
        }
        controls.set(CONTROLS_PAGE, page);
        controls.set(CONTROLS_SELECTED, selected);
        options
    }

    fn states(options: Obj, page: usize) -> Vec<i32> {
        options.at(OPTIONS_CONTROLS + page * VECTOR_SIZE).vec_items::<Button>().map(|b| b[STATE]).collect()
    }

    #[test]
    fn waiting_buttons_without_a_selection_go_idle() {
        let mut arena = Arena::default();
        let o = options(&mut arena, &[&[CONTROL_WAITING, CONTROL_IDLE], &[CONTROL_WAITING]], 0, -1);
        release_stray_waits(o);
        assert_eq!(states(o, 0), [CONTROL_IDLE, CONTROL_IDLE]);
        assert_eq!(states(o, 1), [CONTROL_IDLE]);
    }

    #[test]
    fn the_selected_button_keeps_waiting() {
        let mut arena = Arena::default();
        let o = options(&mut arena, &[&[CONTROL_WAITING], &[CONTROL_WAITING, CONTROL_WAITING]], 1, 1);
        release_stray_waits(o);
        assert_eq!(states(o, 0), [CONTROL_IDLE]);
        assert_eq!(states(o, 1), [CONTROL_IDLE, CONTROL_WAITING]);
    }
}
