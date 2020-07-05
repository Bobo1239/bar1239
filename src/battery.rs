use anyhow::Error;
use battery::{units::Energy, Manager, State};
use futures_async_stream::try_stream;

use crate::{Block, BlockData, BlockState};

pub struct BatteryBlock;

impl BatteryBlock {
    pub fn new() -> BatteryBlock {
        // TODO: Return error if no battery present; or maybe return a Result<Option<Block>> or so
        BatteryBlock
    }
}

impl Block for BatteryBlock {
    #[try_stream(boxed, ok = BlockData, error = Error)]
    async fn block_data_stream(&mut self) {
        // Seems like there's no way to get notified about charging state changes so the best we
        // can do is poll continuously; battery internally uses the sysfs which seems to be the only
        // accessible interface from userspace (upower also does the same)
        loop {
            // TODO: Without vec allocation
            let mut batteries = Vec::new();
            let mut discharging = false;
            for battery in Manager::new()?.batteries()? {
                let battery = battery?;
                batteries.push((battery.state_of_charge(), battery.energy_full()));
                discharging |= battery.state() == State::Discharging;
            }

            let total_energy_full: Energy = batteries.iter().map(|x| x.1).sum();
            let final_percent: f32 = batteries
                .iter()
                .map(|b| f32::from(b.0) * f32::from(b.1 / total_energy_full))
                .sum();

            yield BlockData {
                text: format!(
                    "B{}: {}%",
                    if discharging { "↓" } else { "↑" },
                    (final_percent * 100.).round() as u8
                ),
                state: if final_percent < 0.1 {
                    // TODO: Send notification when battery low
                    BlockState::Warning
                } else {
                    BlockState::Normal
                },
            };

            tokio::time::delay_for(std::time::Duration::from_millis(500)).await;
        }
    }
}
