// Lexing and parsing -------------------------------------------------------

/// A delimiter was opened but never closed.
pub const UNCLOSED_DELIMITER: &str = "E0101";
/// A closing delimiter does not match the one it should close.
pub const MISMATCHED_DELIMITER: &str = "E0102";
/// A closing delimiter has no opener.
pub const UNOPENED_DELIMITER: &str = "E0103";
/// A file declares more than one module.
pub const DUPLICATE_MODULE: &str = "E0104";
/// The grammar could not parse a construct.
pub const PARSE_ERROR: &str = "E0105";

// Name resolution ----------------------------------------------------------

/// A name does not resolve in this scope.
pub const UNRESOLVED_NAME: &str = "E0201";
/// A name is declared more than once.
pub const DUPLICATE_NAME: &str = "E0202";
/// An item is not visible here.
pub const PRIVATE_ITEM: &str = "E0203";
/// An import matches more than one item.
pub const AMBIGUOUS_IMPORT: &str = "E0204";
/// A generic parameter lists the same bound twice.
pub const DUPLICATE_BOUND: &str = "E0205";
/// An `extend` block's target and trait are the same type.
pub const SELF_EXTEND: &str = "E0206";
/// `dyn` is applied to something that is not a trait.
pub const DYN_NOT_TRAIT: &str = "E0207";
/// `Self` is written outside any definition.
pub const SELF_UNAVAILABLE: &str = "E0208";

// Type checking ------------------------------------------------------------

/// Two types that must be equal are not.
pub const MISMATCHED_TYPES: &str = "E0301";
/// An integer type was required, but something else was found.
pub const EXPECTED_INTEGER: &str = "E0302";
/// A float type was required, but something else was found.
pub const EXPECTED_FLOAT: &str = "E0303";
/// A type would have to contain itself.
pub const INFINITE_TYPE: &str = "E0304";
/// The left side of an assignment is not a place.
pub const NOT_ASSIGNABLE: &str = "E0305";
/// A field does not exist on a type.
pub const NO_FIELD: &str = "E0306";
/// A field is not visible here.
pub const PRIVATE_FIELD: &str = "E0308";
/// A `match` does not cover every case.
pub const NON_EXHAUSTIVE_MATCH: &str = "E0310";
/// An integer literal does not fit its type.
pub const INTEGER_LITERAL_OUT_OF_RANGE: &str = "E0311";
/// A type contains itself by value.
pub const RECURSIVE_TYPE: &str = "E0312";
/// A struct declares the same field twice.
pub const DUPLICATE_FIELD: &str = "E0313";
/// A variant does not exist on a type.
pub const NO_VARIANT: &str = "E0314";
/// A function declared to have a body has none.
pub const BODILESS_FUNCTION: &str = "E0315";
/// An operand's type is still an unresolved inference variable.
pub const OPERAND_UNKNOWN_TYPE: &str = "E0316";
/// A literal's suffix names no numeric type.
pub const UNKNOWN_LITERAL_SUFFIX: &str = "E0317";
/// A fractional literal carries an integer suffix.
pub const INT_SUFFIX_ON_FLOAT_LITERAL: &str = "E0318";
/// An unsigned type is given a negative literal.
pub const NEGATIVE_LITERAL_FOR_UNSIGNED: &str = "E0319";
/// `any` appears outside a function signature.
pub const ANY_OUTSIDE_SIGNATURE: &str = "E0320";
/// A field would store a reference.
pub const REFERENCE_FIELD: &str = "E0321";
/// `*` is applied to a value that is not a reference or owned pointer.
pub const DEREF_NOT_A_REFERENCE: &str = "E0322";
/// A value is moved out of a reference.
pub const MOVE_OUT_OF_REFERENCE: &str = "E0323";
/// The type being indexed is still unknown.
pub const INDEX_BASE_UNKNOWN: &str = "E0324";
/// A type has no way to be indexed.
pub const NOT_INDEXABLE: &str = "E0325";
/// `new`'s operand would store a reference.
pub const REFERENCE_IN_NEW: &str = "E0326";
/// `new [elem; count]` repeats an owning element.
pub const OWNED_ELEMENT_IN_NEW_ARRAY: &str = "E0327";
/// `.{ .. }` names no struct and the expected type is unknown.
pub const ELIDED_CTOR_UNKNOWN: &str = "E0328";
/// A `{ .. }` literal is used on something that is not a struct.
pub const CTOR_NOT_A_STRUCT: &str = "E0329";
/// A `{ .. }` literal is used where a non-struct type is expected.
pub const NOT_A_STRUCT_LITERAL: &str = "E0330";
/// A struct or variant field is given a value twice.
pub const DUPLICATE_FIELD_VALUE: &str = "E0331";
/// A struct literal is missing declared fields.
pub const MISSING_FIELDS: &str = "E0332";
/// A `.variant` names no enum in scope.
pub const VARIANT_ENUM_UNKNOWN: &str = "E0333";
/// A `.variant`'s base is not a type.
pub const VARIANT_BASE_NOT_A_TYPE: &str = "E0334";
/// A variant is built with the wrong payload shape.
pub const VARIANT_EXPR_PAYLOAD_SHAPE: &str = "E0335";
/// A record variant names a field it does not declare.
pub const RECORD_FIELD_UNKNOWN: &str = "E0336";
/// A record variant is missing declared fields.
pub const VARIANT_MISSING_FIELDS: &str = "E0337";
/// An `if` with no `else` is used for its value.
pub const IF_NO_ELSE_MISMATCH: &str = "E0338";
/// The operand of `?` is still unknown.
pub const TRY_OPERAND_UNKNOWN: &str = "E0339";
/// `?` is applied to something that is not a `Result` or `Option`.
pub const NOT_TRY: &str = "E0340";
/// `?` is used in a definition with no return type to propagate to.
pub const TRY_OUTSIDE: &str = "E0341";
/// `?`'s error cannot propagate out of the enclosing return type.
pub const TRY_RETURN_MISMATCH: &str = "E0342";
/// A cast target is not a primitive type.
pub const CAST_TARGET_NOT_PRIMITIVE: &str = "E0343";
/// A cast source is not a primitive type.
pub const CAST_SOURCE_NOT_PRIMITIVE: &str = "E0344";
/// The type being cast is still unknown.
pub const CAST_OPERAND_UNKNOWN: &str = "E0345";
/// A cast between primitives is not allowed.
pub const CAST_NOT_ALLOWED: &str = "E0346";
/// A string literal pattern is used.
pub const STRING_PATTERN_UNSUPPORTED: &str = "E0347";
/// The type a variant pattern matches is still unknown.
pub const VARIANT_TYPE_UNKNOWN: &str = "E0348";
/// A record pattern names a field its variant does not declare.
pub const NO_PAYLOAD_FIELD: &str = "E0349";
/// A `match` needs a wildcard arm.
pub const MATCH_NEEDS_WILDCARD: &str = "E0350";
/// A refutable pattern is used in a `let` with no `else`.
pub const REFUTABLE_LET_WITHOUT_ELSE: &str = "E0351";
/// An irrefutable pattern is used in a `let` with an `else`.
pub const IRREFUTABLE_LET_WITH_ELSE: &str = "E0352";
/// A pattern's payload shape does not match the variant's.
pub const PATTERN_PAYLOAD_SHAPE: &str = "E0353";
/// A type is given generic arguments when it takes none.
pub const UNEXPECTED_GENERIC_ARGS: &str = "E0354";
/// A type is given the wrong number of generic arguments.
pub const GENERIC_ARG_COUNT: &str = "E0355";
/// A trait is used as a type on its own.
pub const TRAIT_AS_TYPE: &str = "E0356";
/// A `dyn` type has no size known at compile time.
pub const UNSIZED_DYN: &str = "E0357";
/// An array length is not a constant.
pub const ARRAY_LEN_NOT_CONSTANT: &str = "E0358";
/// An array length is negative.
pub const ARRAY_LEN_NEGATIVE: &str = "E0359";
/// An array length overflows `usize`.
pub const ARRAY_LEN_OVERFLOW: &str = "E0360";
/// An array length divides by zero.
pub const ARRAY_LEN_DIVISION_BY_ZERO: &str = "E0361";
/// A nominal type is instantiated with a reference.
pub const REFERENCE_GENERIC_ARG: &str = "E0362";
/// `Self` is defined in terms of itself.
pub const SELF_CYCLE: &str = "E0363";
/// A crate has no `main` function.
pub const MISSING_MAIN: &str = "E0364";
/// More than one `main` function is in scope.
pub const AMBIGUOUS_MAIN: &str = "E0365";
/// `main` takes parameters.
pub const MAIN_TAKES_PARAMETERS: &str = "E0366";
/// `main` returns a value.
pub const MAIN_RETURNS_A_VALUE: &str = "E0367";
/// `main` is generic.
pub const MAIN_IS_GENERIC: &str = "E0368";
/// A binding is assigned through but not declared `mut`.
pub const NOT_MUTABLE: &str = "E0369";

// Trait solving ------------------------------------------------------------

/// A method does not exist on a type.
pub const NO_METHOD: &str = "E0401";
/// A call has the wrong number of arguments.
pub const WRONG_ARG_COUNT: &str = "E0402";
/// A method name is declared by more than one trait in scope.
pub const AMBIGUOUS_METHOD: &str = "E0403";
/// A function taking no receiver is called on a value.
pub const NO_RECEIVER: &str = "E0404";
/// A receiver cannot provide the mode the method requires.
pub const RECEIVER_MODE: &str = "E0405";
/// A method requiring a reference is called on a temporary.
pub const RECEIVER_NOT_A_PLACE: &str = "E0406";
/// A field is named where a method exists.
pub const FIELD_IS_A_METHOD: &str = "E0407";
/// A value is called but is not callable.
pub const NOT_CALLABLE: &str = "E0408";
/// The type of a method's receiver is still unknown.
pub const RECEIVER_TYPE_UNKNOWN: &str = "E0409";
/// A `self`-by-value method is called through a `dyn` receiver.
pub const DYN_SELF_BY_VALUE: &str = "E0410";
/// A `dyn` method mentions `Self` outside its receiver.
pub const DYN_METHOD_MENTIONS_SELF: &str = "E0411";
/// Two `extend` blocks conflict.
pub const CONFLICTING_EXTENDS: &str = "E0412";
/// Two methods of the same name are declared for one type.
pub const DUPLICATE_METHOD: &str = "E0413";
/// An `extend` target is a trait.
pub const EXTEND_TRAIT: &str = "E0414";
/// An `extend` target is generic.
pub const EXTEND_GENERIC: &str = "E0415";
/// An `extend` target is `any`.
pub const EXTEND_ANY: &str = "E0416";
/// An `extend` target is `dyn`.
pub const EXTEND_DYN: &str = "E0417";
/// An `extend` target has no known size.
pub const EXTEND_UNSIZED: &str = "E0418";
/// An `extend` target is a bare `Self`.
pub const EXTEND_BARE_SELF: &str = "E0419";
/// An `extend` block names something that is not a trait.
pub const EXTEND_WITH_NON_TRAIT: &str = "E0420";
/// An `extend` block does not implement every required method.
pub const MISSING_METHODS: &str = "E0421";
/// An `extend` block declares something the trait does not.
pub const NOT_A_MEMBER: &str = "E0422";
/// An implementation declares the wrong number of generics.
pub const GENERIC_COUNT: &str = "E0423";
/// An implementation's receiver mode does not match the trait's.
pub const SELF_MODE: &str = "E0424";
/// An implementation's parameter count does not match the trait's.
pub const PARAM_COUNT: &str = "E0425";
/// An implementation's parameter type does not match the trait's.
pub const PARAM_TY: &str = "E0426";
/// An implementation's return type does not match the trait's.
pub const RET_TY: &str = "E0427";
/// A required trait bound is not satisfied.
pub const UNSATISFIED_BOUND: &str = "E0428";
/// A bound's type is still unknown.
pub const BOUNDS_ANNOTATIONS_NEEDED: &str = "E0429";
/// An operator is applied to a type with no matching trait implementation.
pub const OPERATOR_TRAIT_MISSING: &str = "E0430";
/// A bound names something that is not a trait.
pub const BOUND_IS_NOT_A_TRAIT: &str = "E0431";
/// A declared bound has the wrong number of type arguments.
pub const ARG_COUNT_MISMATCH: &str = "E0432";

// MIR analysis -------------------------------------------------------------

/// A moved value is used again.
pub const USE_OF_MOVED_VALUE: &str = "E0501";
/// A value cannot be moved out of an array.
pub const MOVE_OUT_OF_ARRAY: &str = "E0502";
/// A place is borrowed while another borrow is live.
pub const EXCLUSIVITY_VIOLATION: &str = "E0503";
/// A closure captures a reference.
pub const CAPTURED_REFERENCE: &str = "E0504";
/// A value is moved out of a closure's environment.
pub const MOVE_OUT_OF_ENVIRONMENT: &str = "E0505";
/// A function body can fall off its end without returning.
pub const NOT_ALL_PATHS_RETURN: &str = "E0506";
/// A binding is never read.
pub const NEVER_READ: &str = "W0501";

// Driver and lang items ----------------------------------------------------

/// The core library does not declare a required lang item.
pub const MISSING_LANG_ITEM: &str = "E0601";
