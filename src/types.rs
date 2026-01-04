use std::fmt;

pub type MarketId = String;
pub type OrderId = u64;
pub type UserId = u64;
pub type Price = u64;
pub type Quantity = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Bid,
    Ask,
}

impl Side {
    pub fn opposite(self) -> Self {
        match self {
            Self::Bid => Self::Ask,
            Self::Ask => Self::Bid,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeInForce {
    /// Good-til-cancelled: match, then rest on book.
    Gtc,
    /// Immediate-or-cancel: match, then cancel remainder.
    Ioc,
    /// Fill-or-kill: must fully fill immediately or reject.
    Fok,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NewOrder {
    Limit {
        user_id: UserId,
        side: Side,
        price: Price,
        quantity: Quantity,
        time_in_force: TimeInForce,
    },
    Market {
        user_id: UserId,
        side: Side,
        quantity: Quantity,
        time_in_force: TimeInForce,
    },
}

impl NewOrder {
    pub fn user_id(&self) -> UserId {
        match *self {
            Self::Limit { user_id, .. } | Self::Market { user_id, .. } => user_id,
        }
    }

    pub fn side(&self) -> Side {
        match *self {
            Self::Limit { side, .. } | Self::Market { side, .. } => side,
        }
    }

    pub fn quantity(&self) -> Quantity {
        match *self {
            Self::Limit { quantity, .. } | Self::Market { quantity, .. } => quantity,
        }
    }

    pub fn time_in_force(&self) -> TimeInForce {
        match *self {
            Self::Limit { time_in_force, .. } | Self::Market { time_in_force, .. } => time_in_force,
        }
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Side::Bid => write!(f, "bid"),
            Side::Ask => write!(f, "ask"),
        }
    }
}
