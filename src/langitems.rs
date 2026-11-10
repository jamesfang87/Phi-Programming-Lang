pub mod ast;
pub mod hir;

/// One definition in the core library that the compiler knows by name.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum LangItem {
    /// `core::option::Option`, which `for` loops and `Iterator` are defined in terms of.
    Option,
    /// `core::result::Result`, which `?` is defined in terms of.
    Result,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Neg,
    Not,
    Eq,
    Comparable,
    Index,
    IndexSet,
    Drop,
    Copy,
    /// `core::iter::Iterator`, which is used to desugar `for` loops
    Iterator,
    /// `core::mem::MaybeUninit`, whose layout is its argument's but which is never dropped.
    MaybeUninit,
    /// `core::llvm::slice_len`, which reads the length stored in a slice's fat pointer.
    SliceLen,
    /// `core::llvm::str_len`, which reads the length stored in a `str`'s fat pointer.
    StrLen,
    /// `core::llvm::swap`, which exchanges the values two references point at.
    Swap,
    /// `core::llvm::replace`, which stores a value through a reference and returns the old one.
    Replace,
    /// `core::llvm::write`, which stores a value into uninitialized storage.
    WriteUninit,
    /// `core::llvm::read`, which moves a value out of initialized storage.
    ReadUninit,
    /// `core::llvm::assume_init_ref`, which borrows initialized storage.
    AssumeInitRef,
    /// `core::llvm::add`, the LLVM `add` instruction.
    LlvmAdd,
    /// `core::llvm::sub`, the LLVM `sub` instruction.
    LlvmSub,
    /// `core::llvm::mul`, the LLVM `mul` instruction.
    LlvmMul,
    /// `core::llvm::div`, the LLVM division instruction.
    LlvmDiv,
    /// `core::llvm::rem`, the LLVM remainder instruction.
    LlvmRem,
    /// `core::llvm::neg`, the LLVM negation instruction.
    LlvmNeg,
    /// `core::llvm::not`, the LLVM boolean negation.
    LlvmNot,
    /// `core::llvm::eq`, the LLVM equality comparison.
    LlvmEq,
    /// `core::llvm::less_than`, the LLVM less-than comparison.
    LlvmLessThan,
    /// `core::llvm::f64_to_i64`, the truncating float-to-integer conversion.
    LlvmF64ToI64,
    /// `core::llvm::i64_to_f64`, the integer-to-float conversion.
    LlvmI64ToF64,
    /// `core::llvm::encode_utf8`, which writes a character's UTF-8 bytes.
    LlvmEncodeUtf8,
    /// `core::libc::malloc`.
    LibcMalloc,
    /// `core::libc::free`.
    LibcFree,
    /// `core::libc::realloc`.
    LibcRealloc,
    /// `core::libc::memcpy`.
    LibcMemcpy,
    /// `core::libc::memset`.
    LibcMemset,
    /// `core::libc::memcmp`.
    LibcMemcmp,
    /// `core::libc::read`.
    LibcRead,
    /// `core::libc::write`.
    LibcWrite,
    /// `core::libc::sizeof`.
    LibcSizeof,
    /// `core::libc::alignof`.
    LibcAlignof,
}

impl LangItem {
    pub const ALL: &'static [LangItem] = &[
        LangItem::Option,
        LangItem::Result,
        LangItem::Add,
        LangItem::Sub,
        LangItem::Mul,
        LangItem::Div,
        LangItem::Rem,
        LangItem::Neg,
        LangItem::Not,
        LangItem::Eq,
        LangItem::Comparable,
        LangItem::Index,
        LangItem::IndexSet,
        LangItem::Drop,
        LangItem::Copy,
        LangItem::Iterator,
        LangItem::MaybeUninit,
        LangItem::SliceLen,
        LangItem::StrLen,
        LangItem::Swap,
        LangItem::Replace,
        LangItem::WriteUninit,
        LangItem::ReadUninit,
        LangItem::AssumeInitRef,
        LangItem::LlvmAdd,
        LangItem::LlvmSub,
        LangItem::LlvmMul,
        LangItem::LlvmDiv,
        LangItem::LlvmRem,
        LangItem::LlvmNeg,
        LangItem::LlvmNot,
        LangItem::LlvmEq,
        LangItem::LlvmLessThan,
        LangItem::LlvmF64ToI64,
        LangItem::LlvmI64ToF64,
        LangItem::LlvmEncodeUtf8,
        LangItem::LibcMalloc,
        LangItem::LibcFree,
        LangItem::LibcRealloc,
        LangItem::LibcMemcpy,
        LangItem::LibcMemset,
        LangItem::LibcMemcmp,
        LangItem::LibcRead,
        LangItem::LibcWrite,
        LangItem::LibcSizeof,
        LangItem::LibcAlignof,
    ];

    /// Returns whether the compiler lowers this lang item's function itself, which is the only
    /// way a function may be declared without a body.
    pub fn is_intrinsic(self) -> bool {
        self.is_llvm_intrinsic()
            || self.is_libc_intrinsic()
            || matches!(
                self,
                LangItem::SliceLen
                    | LangItem::StrLen
                    | LangItem::Swap
                    | LangItem::Replace
                    | LangItem::WriteUninit
                    | LangItem::ReadUninit
                    | LangItem::AssumeInitRef
            )
    }

    /// Returns whether only the compiler's own libraries may call this intrinsic.
    pub fn is_library_only(self) -> bool {
        self.is_untrusted_intrinsic()
            || self.is_llvm_intrinsic()
            || (self.is_libc_intrinsic()
                && !matches!(self, LangItem::LibcSizeof | LangItem::LibcAlignof))
    }

    /// Returns whether this intrinsic is a directly compiled primitive operation.
    pub fn is_llvm_intrinsic(self) -> bool {
        matches!(
            self,
            LangItem::LlvmAdd
                | LangItem::LlvmSub
                | LangItem::LlvmMul
                | LangItem::LlvmDiv
                | LangItem::LlvmRem
                | LangItem::LlvmNeg
                | LangItem::LlvmNot
                | LangItem::LlvmEq
                | LangItem::LlvmLessThan
                | LangItem::LlvmF64ToI64
                | LangItem::LlvmI64ToF64
                | LangItem::LlvmEncodeUtf8
        )
    }

    /// Returns whether this intrinsic is a C library function.
    pub fn is_libc_intrinsic(self) -> bool {
        matches!(
            self,
            LangItem::LibcMalloc
                | LangItem::LibcFree
                | LangItem::LibcRealloc
                | LangItem::LibcMemcpy
                | LangItem::LibcMemset
                | LangItem::LibcMemcmp
                | LangItem::LibcRead
                | LangItem::LibcWrite
                | LangItem::LibcSizeof
                | LangItem::LibcAlignof
        )
    }

    /// Returns whether this intrinsic can read or create uninitialized memory.
    pub fn is_untrusted_intrinsic(self) -> bool {
        matches!(
            self,
            LangItem::WriteUninit | LangItem::ReadUninit | LangItem::AssumeInitRef
        )
    }

    pub fn path(self) -> &'static [&'static str] {
        match self {
            LangItem::Option => &["core", "option", "Option"],
            LangItem::Result => &["core", "result", "Result"],
            LangItem::Add => &["core", "ops", "Add"],
            LangItem::Sub => &["core", "ops", "Sub"],
            LangItem::Mul => &["core", "ops", "Mul"],
            LangItem::Div => &["core", "ops", "Div"],
            LangItem::Rem => &["core", "ops", "Rem"],
            LangItem::Neg => &["core", "ops", "Neg"],
            LangItem::Not => &["core", "ops", "Not"],
            LangItem::Eq => &["core", "ops", "Eq"],
            LangItem::Comparable => &["core", "ops", "Comparable"],
            LangItem::Index => &["core", "ops", "Index"],
            LangItem::IndexSet => &["core", "ops", "IndexSet"],
            LangItem::Drop => &["core", "ops", "Drop"],
            LangItem::Copy => &["core", "ops", "Copy"],
            LangItem::Iterator => &["core", "iter", "Iterator"],
            LangItem::MaybeUninit => &["core", "mem", "MaybeUninit"],
            LangItem::SliceLen => &["core", "llvm", "slice_len"],
            LangItem::StrLen => &["core", "llvm", "str_len"],
            LangItem::Swap => &["core", "llvm", "swap"],
            LangItem::Replace => &["core", "llvm", "replace"],
            LangItem::WriteUninit => &["core", "llvm", "write"],
            LangItem::ReadUninit => &["core", "llvm", "read"],
            LangItem::AssumeInitRef => &["core", "llvm", "assume_init_ref"],
            LangItem::LlvmAdd => &["core", "llvm", "add"],
            LangItem::LlvmSub => &["core", "llvm", "sub"],
            LangItem::LlvmMul => &["core", "llvm", "mul"],
            LangItem::LlvmDiv => &["core", "llvm", "div"],
            LangItem::LlvmRem => &["core", "llvm", "rem"],
            LangItem::LlvmNeg => &["core", "llvm", "neg"],
            LangItem::LlvmNot => &["core", "llvm", "not"],
            LangItem::LlvmEq => &["core", "llvm", "eq"],
            LangItem::LlvmLessThan => &["core", "llvm", "less_than"],
            LangItem::LlvmF64ToI64 => &["core", "llvm", "f64_to_i64"],
            LangItem::LlvmI64ToF64 => &["core", "llvm", "i64_to_f64"],
            LangItem::LlvmEncodeUtf8 => &["core", "llvm", "encode_utf8"],
            LangItem::LibcMalloc => &["core", "libc", "malloc"],
            LangItem::LibcFree => &["core", "libc", "free"],
            LangItem::LibcRealloc => &["core", "libc", "realloc"],
            LangItem::LibcMemcpy => &["core", "libc", "memcpy"],
            LangItem::LibcMemset => &["core", "libc", "memset"],
            LangItem::LibcMemcmp => &["core", "libc", "memcmp"],
            LangItem::LibcRead => &["core", "libc", "read"],
            LangItem::LibcWrite => &["core", "libc", "write"],
            LangItem::LibcSizeof => &["core", "libc", "sizeof"],
            LangItem::LibcAlignof => &["core", "libc", "alignof"],
        }
    }

    pub fn display_path(self) -> String {
        self.path().join("::")
    }
}
