use crate::config::Config;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Closing,
    Holding,
    Opening,
    CloseOnly,
    OpenOnly,
}

#[derive(Debug, Clone)]
pub struct Simulation {
    pub phase: Phase,
    started_at: Instant,
    open_angle: f64,
    closed_angle: f64,
    close_duration: Duration,
    hold_duration: Duration,
    open_duration: Duration,
}

impl Simulation {
    pub fn new(config: &Config) -> Self {
        let mut sim = Self {
            phase: Phase::Idle,
            started_at: Instant::now(),
            open_angle: 0.0,
            closed_angle: 0.0,
            close_duration: Duration::ZERO,
            hold_duration: Duration::ZERO,
            open_duration: Duration::ZERO,
        };
        sim.apply_config(config);
        sim
    }

    pub fn apply_config(&mut self, config: &Config) {
        self.open_angle = config.open_angle;
        self.closed_angle = config.closed_angle;
        self.close_duration = Duration::from_millis(config.close_duration_ms.max(1));
        self.hold_duration = Duration::from_millis(config.hold_duration_ms);
        self.open_duration = Duration::from_millis(config.open_duration_ms.max(1));
    }

    pub fn play_full(&mut self) {
        self.phase = Phase::Closing;
        self.started_at = Instant::now();
    }

    pub fn play_close_only(&mut self) {
        self.phase = Phase::CloseOnly;
        self.started_at = Instant::now();
    }

    pub fn play_open_only(&mut self) {
        self.phase = Phase::OpenOnly;
        self.started_at = Instant::now();
    }

    pub fn stop(&mut self) {
        self.phase = Phase::Idle;
    }

    pub fn is_running(&self) -> bool {
        self.phase != Phase::Idle
    }

    pub fn advance(&mut self, now: Instant) {
        loop {
            let phase_duration = match self.phase {
                Phase::Idle => return,
                Phase::Closing | Phase::CloseOnly => self.close_duration,
                Phase::Holding => self.hold_duration,
                Phase::Opening | Phase::OpenOnly => self.open_duration,
            };
            if now.duration_since(self.started_at) < phase_duration {
                return;
            }
            self.started_at += phase_duration;
            self.phase = match self.phase {
                Phase::Closing => Phase::Holding,
                Phase::Holding => Phase::Opening,
                Phase::Opening | Phase::CloseOnly | Phase::OpenOnly => Phase::Idle,
                Phase::Idle => return,
            };
        }
    }

    pub fn current_angle(&self, now: Instant) -> Option<f64> {
        if self.phase == Phase::Idle {
            return None;
        }
        let elapsed = now.duration_since(self.started_at).as_secs_f64();
        let t = match self.phase {
            Phase::Closing | Phase::CloseOnly => {
                (elapsed / self.close_duration.as_secs_f64().max(1e-6)).clamp(0.0, 1.0)
            }
            Phase::Holding => 1.0,
            Phase::Opening | Phase::OpenOnly => {
                1.0 - (elapsed / self.open_duration.as_secs_f64().max(1e-6)).clamp(0.0, 1.0)
            }
            Phase::Idle => return None,
        };
        Some(self.open_angle + (self.closed_angle - self.open_angle) * t)
    }
}