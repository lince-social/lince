//! Reduced, exact rates for schedule demand accounting.

use std::{cmp::Ordering, num::NonZeroU64};

use serde::{Deserialize, Serialize};

use crate::KarmaBoundaryError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename(serialize = "RationalRateWire"), try_from = "RationalRateWire")]
pub struct RationalRate {
    numerator: u64,
    denominator: u64,
}

impl RationalRate {
    pub fn new(numerator: u64, denominator: u64) -> Result<Self, KarmaBoundaryError> {
        let denominator = NonZeroU64::new(denominator).ok_or_else(|| {
            KarmaBoundaryError::invalid_input("rational denominator must be non-zero")
        })?;
        Ok(Self::with_denominator(numerator, denominator))
    }

    pub(super) fn with_denominator(numerator: u64, denominator: NonZeroU64) -> Self {
        let denominator = denominator.get();
        let divisor = gcd(numerator, denominator);
        Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        }
    }

    pub const fn numerator(self) -> u64 {
        self.numerator
    }

    pub const fn denominator(self) -> u64 {
        self.denominator
    }

    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        let divisor = gcd(self.denominator, rhs.denominator);
        let left_multiplier = rhs.denominator / divisor;
        let right_multiplier = self.denominator / divisor;
        let numerator = self
            .numerator
            .checked_mul(left_multiplier)?
            .checked_add(rhs.numerator.checked_mul(right_multiplier)?)?;
        let denominator = self.denominator.checked_mul(left_multiplier)?;
        Self::new(numerator, denominator).ok()
    }

    pub fn checked_mul_u64(self, multiplier: u64) -> Option<Self> {
        let divisor = gcd(multiplier, self.denominator);
        let reduced_multiplier = multiplier / divisor;
        let denominator = self.denominator / divisor;
        // Cancelling the multiplier against the denominator leaves a reduced rate.
        Some(Self {
            numerator: self.numerator.checked_mul(reduced_multiplier)?,
            denominator,
        })
    }
}

impl PartialOrd for RationalRate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RationalRate {
    fn cmp(&self, other: &Self) -> Ordering {
        (u128::from(self.numerator) * u128::from(other.denominator))
            .cmp(&(u128::from(other.numerator) * u128::from(self.denominator)))
    }
}

#[derive(Deserialize)]
struct RationalRateWire {
    numerator: u64,
    denominator: u64,
}

impl TryFrom<RationalRateWire> for RationalRate {
    type Error = KarmaBoundaryError;

    fn try_from(wire: RationalRateWire) -> Result<Self, Self::Error> {
        Self::new(wire.numerator, wire.denominator)
    }
}

const fn gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}
