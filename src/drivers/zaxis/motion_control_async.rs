// SPDX-License-Identifier: GPL-3.mc-or-later

use core::cell::Cell;

use embassy::channel::signal::Signal;

use crate::util::SharedWithInterrupt;

use super::{Steps, MotionControl, BottomSensor};

pub struct MotionControlAsync {
    inner: SharedWithInterrupt<MotionControl>,
    pub bottom_sensor: BottomSensor,

    signal_on_event: Cell<Option<Event>>,
    signal: Signal<()>,
}

impl MotionControlAsync {
    pub fn new(motion_control: SharedWithInterrupt<MotionControl>, bottom_sensor: BottomSensor) -> Self {
        Self {
            inner: motion_control,
            bottom_sensor,
            signal_on_event: Cell::new(None),
            signal: Signal::new(),
        }
    }

    pub fn on_interrupt(&mut self) {
        let interrupt_fn = |mc: &mut MotionControl| {
            mc.on_interrupt();

            if let Some(event) = self.signal_on_event.get() {
                if event.reached(&self) {
                    self.signal_on_event.set(None);
                    self.signal.signal(());
                }
            }
        };

        unsafe { self.inner.lock_from_interrupt(interrupt_fn) };
    }

    pub async fn wait(&mut self, event: Event) {
        let should_wait = self.inner.lock(|_| {
            // We use the lock here because we need to atomically check for the
            // condition, and set the signal condition for the interrupt handler.
            if event.reached(self) {
                false
            } else {
                self.signal_on_event.set(Some(event));
                self.signal.reset();
                true
            }
        });

        if should_wait {
            self.signal.wait().await;
        }
    }

    // The following are pass-through methods

    pub fn set_target_relative(&mut self, steps: Steps) {
        self.inner.lock(|mc| mc.set_target_relative(steps))
    }

    pub fn set_target(&mut self, target: Steps) {
        self.inner.lock(|mc| mc.set_target(target))
    }

    pub fn stop(&mut self) {
        self.inner.lock(|mc| mc.stop())
    }

    pub fn set_max_speed(&mut self, max_speed: Steps) {
        self.inner.lock(|mc| mc.set_max_speed(max_speed))
    }

    pub fn get_max_speed(&self) -> Steps {
        self.inner.lock(|mc| mc.get_max_speed())
    }

    pub fn get_current_position(&self) -> Steps {
        self.inner.lock(|mc| mc.get_current_position())
    }

    pub fn set_origin(&mut self, origin_position: Steps) {
        self.inner.lock(|mc| mc.set_origin(origin_position))
    }

    pub fn hard_stop(&mut self) {
        self.inner.lock(|mc| mc.hard_stop())
    }

    pub fn is_idle(&self) -> bool {
        self.inner.lock(|mc| mc.is_idle())
    }
}

#[derive(Clone, Copy)]
pub enum Event {
    Idle,
    BottomSensor(bool),
}

impl Event {
    pub fn reached(&self, mc: &MotionControlAsync) -> bool {
        use Event::*;
        match self {
            Idle => mc.is_idle(),
            BottomSensor(value) => mc.bottom_sensor.active() == *value,
        }
    }
}
