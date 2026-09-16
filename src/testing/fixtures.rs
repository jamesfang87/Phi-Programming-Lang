/// Traits and primitive impls that operator-lowering tests need but that the minimal
/// single-file fixtures do not declare. Prepended as one extra core file.
pub const OPS_PREAMBLE: &str = "module core::ops;
     public trait Add { fun add(&self, other: &Self) -> Self; }
     public trait Sub { fun sub(&self, other: &Self) -> Self; }
     public trait Mul { fun mul(&self, other: &Self) -> Self; }
     public trait Div { fun div(&self, other: &Self) -> Self; }
     public trait Rem { fun rem(&self, other: &Self) -> Self; }
     public trait Eq { fun eq(&self, other: &Self) -> bool; }
     public trait Comparable { fun less_than(&self, other: &Self) -> bool; }
     public trait Not { fun not(&self) -> Self; }
     public trait Copy { fun copy(&self) -> Self; }
     extend bool with Not { fun not(&self) -> Self { return !*self; } }
     extend bool with Copy { fun copy(&self) -> Self { return *self; } }
     extend i32 with Copy { fun copy(&self) -> Self { return *self; } }
     extend f64 with Copy { fun copy(&self) -> Self { return *self; } }
     extend i32 with Add { fun add(&self, other: &Self) -> Self { return *self + *other; } }
     extend i32 with Sub { fun sub(&self, other: &Self) -> Self { return *self - *other; } }
     extend i32 with Mul { fun mul(&self, other: &Self) -> Self { return *self * *other; } }
     extend i32 with Div { fun div(&self, other: &Self) -> Self { return *self / *other; } }
     extend i32 with Rem { fun rem(&self, other: &Self) -> Self { return *self % *other; } }
     extend i32 with Eq { fun eq(&self, other: &Self) -> bool { return *self == *other; } }
     extend i32 with Comparable { fun less_than(&self, other: &Self) -> bool { return *self < *other; } }
     extend f64 with Add { fun add(&self, other: &Self) -> Self { return *self + *other; } }
     extend f64 with Sub { fun sub(&self, other: &Self) -> Self { return *self - *other; } }
     extend f64 with Mul { fun mul(&self, other: &Self) -> Self { return *self * *other; } }
     extend f64 with Div { fun div(&self, other: &Self) -> Self { return *self / *other; } }
     extend f64 with Rem { fun rem(&self, other: &Self) -> Self { return *self % *other; } }
     extend f64 with Eq { fun eq(&self, other: &Self) -> bool { return *self == *other; } }
     extend f64 with Comparable { fun less_than(&self, other: &Self) -> bool { return *self < *other; } }
     ";
