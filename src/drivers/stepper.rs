
use stm32f1xx_hal::{
    prelude::*,
    gpio::*,
    gpio::gpioa::*,
    gpio::gpiob::*,
    gpio::gpioc::*,
    gpio::gpioe::*,
    timer::{Timer, Tim2NoRemap, Event, CountDownTimer},
    afio::MAPR,
    pac::{TIM2, TIM7},
    pwm::Channel,
};

use ramp_maker::MotionProfile;
use crate::{consts::stepper::*, runtime::debug, drivers::clock::delay_ns};
const STEPS_PER_MM: f32 = (DRIVER_MICROSTEPS * FULL_STEPS_PER_REVOLUTION) as f32 / SCREW_THREAD_PITCH_MM;

#[derive(PartialEq, PartialOrd, Clone, Copy)]
pub struct Steps(pub i32);

impl Steps {
    pub const MIN: Self = Self(i32::MIN/2);
    pub const MAX: Self = Self(i32::MAX/2);

    pub fn as_mm(self) -> f32 {
        (self.0 as f32) / STEPS_PER_MM
    }
}

impl core::ops::Add for Steps {
    type Output = Steps;
    fn add(self, rhs: Self) -> Self::Output {
        Steps(self.0 + rhs.0)
    }
}

impl core::ops::Sub for Steps {
    type Output = Steps;
    fn sub(self, rhs: Self) -> Self::Output {
        Steps(self.0 - rhs.0)
    }
}

impl core::ops::Neg for Steps {
    type Output = Steps;

    fn neg(self) -> Self::Output {
        Steps(-self.0)
    }
}

pub mod prelude {
    use super::*;

    pub trait StepsExt {
        fn mm(self) -> Steps;
    }

    impl StepsExt for f32 {
        fn mm(self) -> Steps {
            Steps((self * STEPS_PER_MM) as i32)
        }
    }

    impl StepsExt for i32 {
        fn mm(self) -> Steps {
            (self as f32).mm()
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
pub enum Direction {
    Up,
    Down,
}

use prelude::*;


pub struct Stepper {
    step_timer: CountDownTimer<TIM7>,
    step: PE5<Output<PushPull>>,
    dir: PE4<Output<PushPull>>,
    enable: PE6<Output<PushPull>>,
    profile: ramp_maker::Trapezoidal<f32>,
    pub current_position: Steps,
    pub target: Steps,
    pub max_speed: Steps,
}

impl Stepper {
    pub fn new(
        dir: PE4<Input<Floating>>,
        step: PE5<Input<Floating>>,
        enable: PE6<Input<Floating>>,

        mode0: PC3<Input<Floating>>,
        mode1: PC0<Input<Floating>>,

        decay0: PC1<Input<Floating>>,
        decay1: PC2<Input<Floating>>,

        vref: PA3<Input<Floating>>,
        pwm_timer: Timer<TIM2>, // Or TIM5 in alternate mode.

        step_timer: Timer<TIM7>, // Any timer will do.

        gpioa_crl: &mut Cr<CRL, 'A'>,
        gpioc_crl: &mut Cr<CRL, 'C'>,
        gpioe_crl: &mut Cr<CRL, 'E'>,
        mapr: &mut MAPR,
    ) -> Self {
        // Pins that are related, but usage not known:
        // PB4 input pull up
        // PB3 input pull up
        // PC13 output (1)
        // PA2 output

        let dir = dir.into_push_pull_output(gpioe_crl);
        let step = step.into_push_pull_output(gpioe_crl);
        let enable = enable.into_push_pull_output(gpioe_crl);

        // DIR/MODE changes: wait at least 200ns before STEP changes

        // Mode0 | Mode1 | Step mode
        //-------|-------|-----------
        // 0     |  0    | Full step (100% current)
        // 0     |  330k    | Full step (71% current)
        // 1     |  0    | Non-circular 1/2 step
        // Hi-Z  |  0    | 1/2 step
        // 0     |  1    | 1/4 step
        // 1     |  1    | 1/8 step
        // Hi-Z  |  1    | 1/16 step
        // 0     |  Hi-Z | 1/32 step
        // 0     |  330k GND | 1/64 step
        // Hi-Z  |  Hi-Z | 1/128 step
        // 1     |  Hi-Z | 1/256 step

        // We are going with 1/16 microstepping.
        mode0.into_floating_input(gpioc_crl); // HiZ
        mode1.into_push_pull_output_with_state(gpioc_crl, PinState::High); // 1

        // New decay setting takes 10us to take effect.

        // Decay0 | Decay1 | Increasing Steps          | Decreasing Steps
        // -------|--------|---------------------------|----------------------------
        //  0     |   0    | Smart tune Dynamic Decay  | Smart tune Dynamic Decay
        //  0     |   1    | Smart tune Ripple Control | Smart tune Ripple Control
        //  1     |   0    | Mixed decay: 30% fast     | Mixed decay: 30% fast
        //  1     |   1    | Slow decay                | Mixed decay: 30% fast
        //  Hi-Z  |   0    | Mixed decay: 60% fast     | Mixed decay: 60% fast
        //  Hi-Z  |   1    | Slow decay                | Slow decay

        decay0.into_push_pull_output_with_state(gpioc_crl, PinState::Low);
        decay1.into_push_pull_output_with_state(gpioc_crl, PinState::Low);

        let vref = vref.into_alternate_push_pull(gpioa_crl);


        // TIMER2 (or timer5 remapped), CH4
        let mut pwm = pwm_timer.pwm::<Tim2NoRemap, _, _, _>(vref, mapr, 100.khz());
        pwm.set_duty(Channel::C4, (pwm.get_max_duty() * 8) / 10);
        pwm.enable(Channel::C4);

        let profile = ramp_maker::Trapezoidal::new(MAX_ACCELERATION.mm().0 as f32);

        // Value doesn't matter here, it will be re-initialized later.
        let step_timer = step_timer.start_count_down(10.hz());

        let current_position = Steps(0);
        let target = Steps(0);
        let max_speed = DEFAULT_MAX_SPEED.mm();

        Self { step_timer, step, dir, enable, profile, current_position, max_speed, target }
    }

    pub fn on_interrupt(&mut self) {
        let _ = self.step_timer.wait(); // clears the interrupt flag
        self.do_step();

        if let Some(delay_s) = self.profile.next_delay() {
            let delay_us = (delay_s * 1_000_000.0) as u32;
            self.reload_timer(delay_us, true);
        } else {
            self.stop();
        }
    }

    fn reload_timer(&mut self, mut delay_us: u32, since_last_interrupt: bool) {
        if since_last_interrupt {
            delay_us = delay_us.checked_sub(self.step_timer.micros_since()).unwrap_or(0);
        }
        if delay_us == 0 {
            delay_us = 1;
        }
        self.step_timer.start(delay_us.us());
    }

    fn do_step(&mut self) {
        // The stepper motor advances when the `step` pin rises from low to high.
        // We have to hold the `step` pin high for at least 1us according to the datasheet.
        self.step.set_high();
        cortex_m::asm::delay(120);
        self.step.set_low();

        self.current_position.0 += match self.current_direction() {
            Direction::Up => 1,
            Direction::Down => -1,
        }
    }

    fn current_direction(&self) -> Direction {
        match self.dir.is_set_high() {
            true => Direction::Up,
            false => Direction::Down,
        }
    }

    fn set_direction(&mut self, direction: Direction) {
        match direction {
            Direction::Up => self.dir.set_high(),
            Direction::Down => self.dir.set_low(),
        }
    }

    // If max_speed is None, it goes back to default.
    pub fn set_max_speed(&mut self, max_speed: Option<Steps>) {
        self.max_speed = max_speed.unwrap_or(DEFAULT_MAX_SPEED.mm());

        let mut steps = self.target.0 - self.current_position.0;
        if steps < 0 { steps = -steps; }

        self.profile.enter_position_mode(self.max_speed.0 as f32, steps as u32);
    }

    // to current position
    pub fn set_target_relative(&mut self, steps: Steps) {
        self.set_target(self.current_position + steps);
    }

    pub fn set_target(&mut self, target: Steps) {
        self.target = target;
        let steps = target - self.current_position;

        if steps.0 == 0 {
            return;
        }

        let (dir, steps) = if steps.0 > 0 {
            (Direction::Up, steps.0 as u32)
        } else {
            (Direction::Down, -steps.0 as u32)
        };

        self.set_direction(dir);
        self.profile.enter_position_mode(self.max_speed.0 as f32, steps);

        // We need to hold the enable pin high for 5us before we can start stepping the motor.
        self.enable.set_high();
        self.reload_timer(5, false);
        self.step_timer.listen(Event::Update);
    }

    pub fn set_origin(&mut self, origin_position: Steps) {
        self.target = self.target + self.current_position - origin_position;
        self.current_position = -origin_position;
    }

    pub fn controlled_stop(&mut self) {
        self.profile.enter_position_mode(0.000001.mm().0 as f32, 100.0.mm().0 as u32);
    }

    pub fn stop(&mut self) {
        self.profile = ramp_maker::Trapezoidal::new(MAX_ACCELERATION.mm().0 as f32);
        self.target = self.current_position;

        self.step_timer.unlisten(Event::Update);
        self.step.set_low();
        self.enable.set_low();
    }

    pub fn is_idle(&self) -> bool {
        self.enable.is_set_low()
    }

    /*
    pub fn interruptible(self) -> InterruptibleStepper {
        let mut s = InterruptibleStepper::new(self);
        s.enable_interrupts();
        s
    } */
}

/*
impl InterruptibleStepper {
    pub fn wait_for_completion(&self) {
        while !self.access(|s| { s.is_idle() }) { }
    }
}

// do a macro for this at some point?

static mut INSTANCE: Option<Stepper> = None;

pub struct InterruptibleStepper;

impl InterruptibleStepper {
    fn new(stepper: Stepper) -> Self {
        unsafe {
            assert!(INSTANCE.is_none());
            INSTANCE = Some(stepper);
        }
        Self {}
    }

    pub fn enable_interrupts(&mut self) {
        unsafe { NVIC::unmask(Interrupt::TIM7); }
    }

    pub fn disable_interrupts(&mut self) {
        NVIC::mask(Interrupt::TIM7);
    }

    pub fn access<F, R>(&self, f: F) -> R
        where F: FnOnce(&'static Stepper) -> R
    {
        cortex_m::interrupt::free(|_| {
            let stepper = unsafe { INSTANCE.as_ref().unwrap() };
            f(stepper)
        })
    }

    pub fn modify<F, R>(&mut self, f: F) -> R
        where F: FnOnce(&'static mut Stepper) -> R
    {
        cortex_m::interrupt::free(|_| {
            let stepper = unsafe { INSTANCE.as_mut().unwrap() };
            f(stepper)
        })
    }
}

#[interrupt]
fn TIM7() {
    let stepper = unsafe { INSTANCE.as_mut().unwrap() };
    stepper.interrupt_tim7();
}

*/
