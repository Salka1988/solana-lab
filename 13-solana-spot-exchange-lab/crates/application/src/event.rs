use core::num::NonZeroU128;

use domain::{AssetId, BalanceAmount, Fill, Order, TraderId};

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommandId(NonZeroU128);

impl CommandId {
    pub fn new(value: u128) -> Result<Self, Error> {
        NonZeroU128::new(value)
            .map(Self)
            .ok_or(Error::Domain(domain::Error::ZeroValue))
    }

    pub const fn get(self) -> u128 {
        self.0.get()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    DepositCredited {
        command_id: CommandId,
        trader_id: TraderId,
        asset_id: AssetId,
        amount: BalanceAmount,
    },
    OrderPlaced {
        command_id: CommandId,
        order: Order,
        fills: Vec<Fill>,
    },
}

impl Event {
    pub const fn command_id(&self) -> CommandId {
        match self {
            Self::DepositCredited { command_id, .. } | Self::OrderPlaced { command_id, .. } => {
                *command_id
            }
        }
    }
}
