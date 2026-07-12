use crate::{newtype::non_zero_newtype, Error};
use core::num::NonZeroU64;

non_zero_newtype!(Price, u64, NonZeroU64);
non_zero_newtype!(Quantity, u64, NonZeroU64);
non_zero_newtype!(Amount, u64, NonZeroU64);
non_zero_newtype!(TickSize, u64, NonZeroU64);
non_zero_newtype!(LotSize, u64, NonZeroU64);

impl Amount {
    pub fn checked_add(self, rhs: Self) -> Result<Self, Error> {
        let value = self
            .get()
            .checked_add(rhs.get())
            .ok_or(Error::ArithmeticOverflow)?;
        Self::new(value)
    }

    pub fn checked_sub(self, rhs: Self) -> Result<Self, Error> {
        let value = self
            .get()
            .checked_sub(rhs.get())
            .ok_or(Error::ArithmeticUnderflow)?;
        Self::new(value)
    }
}

impl Price {
    pub fn quote_cost(self, quantity: Quantity) -> Result<u128, Error> {
        u128::from(self.get())
            .checked_mul(u128::from(quantity.get()))
            .ok_or(Error::ArithmeticOverflow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_values_reject_zero() {
        assert_eq!(Price::new(0), Err(Error::ZeroValue));
        assert_eq!(Quantity::new(0), Err(Error::ZeroValue));
        assert_eq!(Amount::new(0), Err(Error::ZeroValue));
        assert_eq!(TickSize::new(0), Err(Error::ZeroValue));
        assert_eq!(LotSize::new(0), Err(Error::ZeroValue));
    }

    #[test]
    fn amount_addition_is_checked() {
        let lhs = Amount::new(10).unwrap();
        let rhs = Amount::new(15).unwrap();

        assert_eq!(lhs.checked_add(rhs).unwrap().get(), 25);
        assert_eq!(
            Amount::new(u64::MAX)
                .unwrap()
                .checked_add(Amount::new(1).unwrap()),
            Err(Error::ArithmeticOverflow)
        );
    }

    #[test]
    fn amount_subtraction_is_checked() {
        let lhs = Amount::new(15).unwrap();
        let rhs = Amount::new(10).unwrap();

        assert_eq!(lhs.checked_sub(rhs).unwrap().get(), 5);
        assert_eq!(rhs.checked_sub(lhs), Err(Error::ArithmeticUnderflow));
    }

    #[test]
    fn quote_cost_uses_widened_math() {
        let price = Price::new(u64::MAX).unwrap();
        let quantity = Quantity::new(u64::MAX).unwrap();

        assert_eq!(
            price.quote_cost(quantity).unwrap(),
            u128::from(u64::MAX) * u128::from(u64::MAX)
        );
    }
}
