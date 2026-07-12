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
