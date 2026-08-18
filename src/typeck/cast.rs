use crate::nameres::PrimTy;
use crate::typeck::unify::{is_float, is_integer};

fn int_width(prim: PrimTy) -> Option<u32> {
    match prim {
        PrimTy::I8 | PrimTy::U8 => Some(8),
        PrimTy::I16 | PrimTy::U16 => Some(16),
        PrimTy::I32 | PrimTy::U32 => Some(32),
        PrimTy::I64 | PrimTy::U64 => Some(64),
        _ => None,
    }
}

fn is_signed(prim: PrimTy) -> bool {
    matches!(prim, PrimTy::I8 | PrimTy::I16 | PrimTy::I32 | PrimTy::I64)
}

/// Checks whether `from as to` is a lossless cast between two distinct primitive types.
pub(crate) fn cast_allowed(from: PrimTy, to: PrimTy) -> Result<(), &'static str> {
    use PrimTy::*;

    if from == to {
        return Ok(());
    }

    if is_integer(from) && is_integer(to) {
        return int_to_int(from, to);
    }

    match (from, to) {
        // `f32`'s 24-bit mantissa holds every 16-bit integer exactly; `f64`'s 53 bits do the
        // same for every 32-bit integer. Wider than that, and some value would round.
        (f, F32) if is_integer(f) && int_width(f) <= Some(16) => Ok(()),
        (f, F64) if is_integer(f) && int_width(f) <= Some(32) => Ok(()),
        (f, F32 | F64) if is_integer(f) => Err(
            "this integer type is wider than the float type's mantissa, so a large enough \
                 value would round",
        ),

        (F32, F64) => Ok(()),
        (F64, F32) => Err("narrows to a smaller float type, which can lose precision"),

        (f, t) if is_float(f) && is_integer(t) => Err(
            "would truncate any fractional part -- there is no truncating cast here, only \
                 lossless ones",
        ),

        (Bool, t) if is_integer(t) || is_float(t) => Ok(()),
        (f, Bool) if is_integer(f) || is_float(f) => {
            Err("not every value of this type is `0` or `1`")
        }

        // A `char` is a Unicode scalar value, `0..=0x10FFFF`, which needs 21 bits: too wide for
        // an 8- or 16-bit integer, but always in range for a 32- or 64-bit one, signed or not.
        (Char, U32 | U64 | I32 | I64) => Ok(()),
        (Char, t) if is_integer(t) => {
            Err("a `char` can hold a codepoint as high as 0x10FFFF, wider than this type")
        }

        // Every `u8` value is a valid Unicode scalar value; the surrogate range
        // `0xD800..=0xDFFF` starts well above `u8::MAX`. No wider integer type has that
        // guarantee.
        (U8, Char) => Ok(()),
        (f, Char) if is_integer(f) => {
            Err("not every value of this type is a valid Unicode scalar value")
        }

        (Bool, Char) | (Char, Bool) => Err("`bool` and `char` share no representation"),
        (Char, F32 | F64) | (F32 | F64, Char) => {
            Err("`char` and a floating-point type share no representation")
        }

        _ => unreachable!(
            "cast_allowed should classify every pair of primitives; missing ({from:?}, {to:?})"
        ),
    }
}

/// Whether narrowing between two integer types loses information. Same-signedness widening is
/// always fine, and unsigned to a strictly wider signed type is too, since an equal-width
/// signed type still cannot hold the unsigned type's top half.
fn int_to_int(from: PrimTy, to: PrimTy) -> Result<(), &'static str> {
    let (from_width, to_width) = (
        int_width(from).expect("caller checked `from` is an integer"),
        int_width(to).expect("caller checked `to` is an integer"),
    );

    match (is_signed(from), is_signed(to)) {
        (true, true) | (false, false) if from_width <= to_width => Ok(()),
        (true, true) | (false, false) => {
            Err("narrows to a smaller integer type, which can silently truncate the value")
        }
        // Unsigned to signed is safe only when the destination is strictly wider: an
        // equal-width signed type still can't hold the unsigned type's top half.
        (false, true) if from_width < to_width => Ok(()),
        (false, true) => Err("a large enough unsigned value would overflow this signed type"),
        // A negative value has no unsigned equivalent, no matter how wide the destination is.
        (true, false) => Err("a negative value has no meaningful unsigned equivalent"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [PrimTy; 12] = [
        PrimTy::I8,
        PrimTy::I16,
        PrimTy::I32,
        PrimTy::I64,
        PrimTy::U8,
        PrimTy::U16,
        PrimTy::U32,
        PrimTy::U64,
        PrimTy::F32,
        PrimTy::F64,
        PrimTy::Bool,
        PrimTy::Char,
    ];

    /// Every one of the 144 ordered pairs is classified one way or the other, which is what
    /// guarantees the `unreachable!()` in `cast_allowed` never fires.
    #[test]
    fn every_pair_of_primitives_is_classified() {
        for &from in &ALL {
            for &to in &ALL {
                let _ = cast_allowed(from, to);
            }
        }
    }

    #[test]
    fn a_type_always_casts_to_itself() {
        for &prim in &ALL {
            assert!(cast_allowed(prim, prim).is_ok(), "{prim:?} as itself");
        }
    }

    #[test]
    fn same_signedness_widening_is_allowed() {
        assert!(cast_allowed(PrimTy::I8, PrimTy::I64).is_ok());
        assert!(cast_allowed(PrimTy::U16, PrimTy::U32).is_ok());
    }

    #[test]
    fn same_signedness_narrowing_is_rejected() {
        assert!(cast_allowed(PrimTy::I64, PrimTy::I8).is_err());
        assert!(cast_allowed(PrimTy::U32, PrimTy::U16).is_err());
    }

    #[test]
    fn unsigned_to_strictly_wider_signed_is_allowed() {
        assert!(cast_allowed(PrimTy::U8, PrimTy::I16).is_ok());
        assert!(cast_allowed(PrimTy::U32, PrimTy::I64).is_ok());
    }

    #[test]
    fn unsigned_to_equal_width_signed_is_rejected() {
        assert!(cast_allowed(PrimTy::U8, PrimTy::I8).is_err());
        assert!(cast_allowed(PrimTy::U64, PrimTy::I64).is_err());
    }

    #[test]
    fn unsigned_to_narrower_signed_is_rejected() {
        assert!(cast_allowed(PrimTy::U32, PrimTy::I16).is_err());
    }

    #[test]
    fn signed_to_unsigned_is_always_rejected() {
        assert!(cast_allowed(PrimTy::I8, PrimTy::U64).is_err());
        assert!(cast_allowed(PrimTy::I64, PrimTy::U64).is_err());
    }

    #[test]
    fn narrow_integers_cast_to_either_float() {
        for int in [PrimTy::I8, PrimTy::I16, PrimTy::U8, PrimTy::U16] {
            assert!(cast_allowed(int, PrimTy::F32).is_ok(), "{int:?} as f32");
            assert!(cast_allowed(int, PrimTy::F64).is_ok(), "{int:?} as f64");
        }
    }

    #[test]
    fn thirty_two_bit_integers_cast_only_to_f64() {
        for int in [PrimTy::I32, PrimTy::U32] {
            assert!(cast_allowed(int, PrimTy::F32).is_err(), "{int:?} as f32");
            assert!(cast_allowed(int, PrimTy::F64).is_ok(), "{int:?} as f64");
        }
    }

    #[test]
    fn sixty_four_bit_integers_cast_to_neither_float() {
        for int in [PrimTy::I64, PrimTy::U64] {
            assert!(cast_allowed(int, PrimTy::F32).is_err(), "{int:?} as f32");
            assert!(cast_allowed(int, PrimTy::F64).is_err(), "{int:?} as f64");
        }
    }

    #[test]
    fn f32_widens_to_f64_but_not_back() {
        assert!(cast_allowed(PrimTy::F32, PrimTy::F64).is_ok());
        assert!(cast_allowed(PrimTy::F64, PrimTy::F32).is_err());
    }

    #[test]
    fn float_to_integer_is_always_rejected() {
        for float in [PrimTy::F32, PrimTy::F64] {
            for int in [
                PrimTy::I8,
                PrimTy::I16,
                PrimTy::I32,
                PrimTy::I64,
                PrimTy::U8,
                PrimTy::U16,
                PrimTy::U32,
                PrimTy::U64,
            ] {
                assert!(cast_allowed(float, int).is_err(), "{float:?} as {int:?}");
            }
        }
    }

    #[test]
    fn bool_casts_to_every_numeric_type() {
        for numeric in [
            PrimTy::I8,
            PrimTy::I16,
            PrimTy::I32,
            PrimTy::I64,
            PrimTy::U8,
            PrimTy::U16,
            PrimTy::U32,
            PrimTy::U64,
            PrimTy::F32,
            PrimTy::F64,
        ] {
            assert!(
                cast_allowed(PrimTy::Bool, numeric).is_ok(),
                "bool as {numeric:?}"
            );
            assert!(
                cast_allowed(numeric, PrimTy::Bool).is_err(),
                "{numeric:?} as bool"
            );
        }
    }

    #[test]
    fn char_casts_only_to_32_or_64_bit_integers() {
        assert!(cast_allowed(PrimTy::Char, PrimTy::I32).is_ok());
        assert!(cast_allowed(PrimTy::Char, PrimTy::I64).is_ok());
        assert!(cast_allowed(PrimTy::Char, PrimTy::U32).is_ok());
        assert!(cast_allowed(PrimTy::Char, PrimTy::U64).is_ok());
        assert!(cast_allowed(PrimTy::Char, PrimTy::I8).is_err());
        assert!(cast_allowed(PrimTy::Char, PrimTy::I16).is_err());
        assert!(cast_allowed(PrimTy::Char, PrimTy::U8).is_err());
        assert!(cast_allowed(PrimTy::Char, PrimTy::U16).is_err());
    }

    #[test]
    fn only_u8_casts_to_char() {
        assert!(cast_allowed(PrimTy::U8, PrimTy::Char).is_ok());
        for int in [
            PrimTy::I8,
            PrimTy::I16,
            PrimTy::I32,
            PrimTy::I64,
            PrimTy::U16,
            PrimTy::U32,
            PrimTy::U64,
        ] {
            assert!(cast_allowed(int, PrimTy::Char).is_err(), "{int:?} as char");
        }
    }

    #[test]
    fn bool_and_char_share_no_representation() {
        assert!(cast_allowed(PrimTy::Bool, PrimTy::Char).is_err());
        assert!(cast_allowed(PrimTy::Char, PrimTy::Bool).is_err());
    }

    #[test]
    fn char_and_float_share_no_representation() {
        assert!(cast_allowed(PrimTy::Char, PrimTy::F32).is_err());
        assert!(cast_allowed(PrimTy::F64, PrimTy::Char).is_err());
    }
}
