// SPDX-License-Identifier: GPL-3.0-or-later

use embassy::channel::signal::Signal;
use lvgl::{
    style::State,
    core::{Display, Screen},
    widgets::*,
    prelude::*,
};
use alloc::format;

use lvgl::cstr_core::CStr;
use crate::drivers::zaxis::{
    prelude::*,
    self,
};
use crate::consts::zaxis::motion_control::*;

pub struct MoveZ {
    btn_move_up: Btn<Self>,
    btn_move_down: Btn<Self>,
    speed_slider: Slider<Self>,
    speed_label: Label<Self>,
    position_label: Label<Self>,
    btn_calibrate: Btn<Self>,

    user_action: &'static Signal<UserAction>,
}

impl MoveZ {
    pub fn new<D>(display: &Display<D>, user_action: &'static Signal<UserAction>) -> Screen::<Self> {
        use lvgl::widgets::*;
        use lvgl::style::*;
        use lvgl::core::*;

        let spacing = 12;

        let mut screen = Screen::<Self>::new(display);

        let btn_move_up = Btn::new(&mut screen).apply(|obj| {
            Label::new(obj)
                .set_text(&CStr::from_bytes_with_nul(b"Move Up\0").unwrap());
            obj
            .align_to(&screen, Align::TopMid, 0, 2*spacing)
            .add_flag(Flag::CHECKABLE)
            .on_event(Event::Clicked, |context| {
                let checked = context.btn_move_up.has_state(State::CHECKED);

                context.btn_move_down.add_state(State::DISABLED);
                context.btn_calibrate.add_state(State::DISABLED);
                if !checked {
                    context.btn_move_up.add_state(State::DISABLED);
                }

                context.user_action.signal(
                    if checked { UserAction::MoveUp }
                    else { UserAction::StopRequested }
                );
            });
        });

        let btn_move_down = Btn::new(&mut screen).apply(|obj| {
            Label::new(obj)
                .set_text(&CStr::from_bytes_with_nul(b"Move Down\0").unwrap());
            obj
            .align_to(&btn_move_up, Align::OutBottomMid, 0, spacing)
            .add_flag(Flag::CHECKABLE)
            .on_event(Event::Clicked, |context| {
                let checked = context.btn_move_down.has_state(State::CHECKED);

                context.btn_move_up.add_state(State::DISABLED);
                context.btn_calibrate.add_state(State::DISABLED);
                if !checked {
                    context.btn_move_down.add_state(State::DISABLED);
                }

                context.user_action.signal(
                    if checked { UserAction::MoveDown }
                    else { UserAction::StopRequested }
                );
            });
        });

        let speed_slider = Slider::new(&mut screen).apply(|obj| { obj
            .align_to(&btn_move_down, Align::OutBottomMid, 0, 2*spacing)
            .set_range(1500, 10_000)
            .set_value(10_000, 0)
            .on_event(Event::ValueChanged, |context| {
                let value = unsafe { lvgl::sys::lv_slider_get_value(context.speed_slider.raw) };

                let value = (value as f32)/10000.0;
                let value = value*value*value;
                let value = value * MAX_SPEED;

                context.user_action.signal(UserAction::SetSpeed(value));
            });
        });

        let speed_label = Label::new(&mut screen).apply(|obj| { obj
            .align_to(&speed_slider, Align::OutBottomLeft, 50, spacing);
        });

        let position_label = Label::new(&mut screen).apply(|obj| { obj
            .align_to(&speed_label, Align::OutBottomLeft, 0, 0);
        });

        let btn_calibrate = Btn::new(&mut screen).apply(|obj| {
            Label::new(obj)
                .set_text(&CStr::from_bytes_with_nul(b"Move to Z=0\0").unwrap());
            obj
            .align_to(&position_label, Align::OutBottomMid, 0, spacing)
            .add_flag(Flag::CHECKABLE)
            .on_event(Event::Clicked, |context| {
                let checked = context.btn_calibrate.has_state(State::CHECKED);

                context.btn_move_up.add_state(State::DISABLED);
                context.btn_move_down.add_state(State::DISABLED);
                if !checked {
                    context.btn_calibrate.add_state(State::DISABLED);
                }

                context.user_action.signal(
                    if checked { UserAction::Calibrate }
                    else { UserAction::StopRequested }
                );
            });
        });

        Label::new(&mut screen).apply(|obj| { obj
            .set_text(&CStr::from_bytes_with_nul(b"Turbo Resin v0.1.3\0").unwrap())
            .align_to(&screen, Align::BottomRight, -5, -5);
        });

        let context = Self { btn_move_up, btn_move_down, speed_slider,
            speed_label, position_label, btn_calibrate, user_action
        };

        screen.apply(|s| {
            s.context().replace(context);
        })
    }

    pub fn update_ui(&mut self, zaxis: &zaxis::MotionControlAsync) {
        if zaxis.is_idle() {
            self.btn_move_up.clear_state(State::CHECKED | State::DISABLED);
            self.btn_move_down.clear_state(State::CHECKED | State::DISABLED);
            self.btn_calibrate.clear_state(State::CHECKED | State::DISABLED);
        }

        // set_text() makes a copy of the string internally.
        self.position_label.set_text(&CStr::from_bytes_with_nul(
            format!("Position: {:.2} mm\0", zaxis.get_current_position().as_mm()).as_bytes()
        ).unwrap());

        self.speed_label.set_text(&CStr::from_bytes_with_nul(
            format!("Max speed: {:.2} mm/s\0", zaxis.get_max_speed().as_mm()).as_bytes()
        ).unwrap());
    }
}

#[derive(Debug)]
pub enum UserAction {
    MoveUp,
    MoveDown,
    Calibrate,
    StopRequested,
    SetSpeed(f32),
}

impl UserAction {
    pub async fn do_user_action(&self, mc: &mut zaxis::MotionControlAsync) {
        use UserAction::*;
        match self {
            MoveUp => mc.set_target_relative(40.0.mm()),
            MoveDown => mc.set_target_relative((-40.0).mm()),
            StopRequested => mc.stop(),
            SetSpeed(v) => mc.set_max_speed(v.mm()),
            Calibrate => {
                zaxis::calibrate_origin(mc, None).await;
                mc.set_max_speed(MAX_SPEED.mm());
                mc.set_target(0.0.mm());
                mc.wait(zaxis::Event::Idle).await;
            }
        }
   }
}
