macro_rules! non_zero_newtype {
    ($name:ident, $inner:ty, $non_zero:ty) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name($non_zero);

        impl $name {
            pub fn new(value: $inner) -> Result<Self, crate::Error> {
                <$non_zero>::new(value)
                    .map(Self)
                    .ok_or(crate::Error::ZeroValue)
            }

            pub const fn get(self) -> $inner {
                self.0.get()
            }
        }
    };
}

pub(crate) use non_zero_newtype;

macro_rules! zeroable_u64_newtype {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u64);

        impl $name {
            pub const ZERO: Self = Self(0);

            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            pub const fn get(self) -> u64 {
                self.0
            }

            pub const fn is_zero(self) -> bool {
                self.0 == 0
            }
        }
    };
}

pub(crate) use zeroable_u64_newtype;
