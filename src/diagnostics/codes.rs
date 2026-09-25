pub const UNCLOSED_DELIMITER: &str = "E0101";

pub const MISMATCHED_DELIMITER: &str = "E0102";

pub const UNOPENED_DELIMITER: &str = "E0103";

pub const DUPLICATE_MODULE: &str = "E0104";

pub const PARSE_ERROR: &str = "E0105";

pub const UNRESOLVED_NAME: &str = "E0201";

pub const DUPLICATE_NAME: &str = "E0202";

pub const PRIVATE_ITEM: &str = "E0203";

pub const AMBIGUOUS_IMPORT: &str = "E0204";

pub const DUPLICATE_BOUND: &str = "E0205";

pub const SELF_EXTEND: &str = "E0206";

pub const DYN_NOT_TRAIT: &str = "E0207";

pub const SELF_UNAVAILABLE: &str = "E0208";

pub const MISMATCHED_TYPES: &str = "E0301";

pub const EXPECTED_INTEGER: &str = "E0302";

pub const EXPECTED_FLOAT: &str = "E0303";

pub const INFINITE_TYPE: &str = "E0304";

pub const NOT_ASSIGNABLE: &str = "E0305";

pub const NO_FIELD: &str = "E0306";

pub const PRIVATE_FIELD: &str = "E0308";

pub const NON_EXHAUSTIVE_MATCH: &str = "E0310";

pub const INTEGER_LITERAL_OUT_OF_RANGE: &str = "E0311";

pub const RECURSIVE_TYPE: &str = "E0312";

pub const DUPLICATE_FIELD: &str = "E0313";

pub const NO_VARIANT: &str = "E0314";

pub const BODILESS_FUNCTION: &str = "E0315";

pub const OPERAND_UNKNOWN_TYPE: &str = "E0316";

pub const UNKNOWN_LITERAL_SUFFIX: &str = "E0317";

pub const INT_SUFFIX_ON_FLOAT_LITERAL: &str = "E0318";

pub const NEGATIVE_LITERAL_FOR_UNSIGNED: &str = "E0319";

pub const ANY_OUTSIDE_SIGNATURE: &str = "E0320";

pub const REFERENCE_FIELD: &str = "E0321";

pub const DEREF_NOT_A_REFERENCE: &str = "E0322";

pub const MOVE_OUT_OF_REFERENCE: &str = "E0323";

pub const INDEX_BASE_UNKNOWN: &str = "E0324";

pub const NOT_INDEXABLE: &str = "E0325";

pub const REFERENCE_IN_NEW: &str = "E0326";

pub const OWNED_ELEMENT_IN_NEW_ARRAY: &str = "E0327";

pub const ELIDED_CTOR_UNKNOWN: &str = "E0328";

pub const CTOR_NOT_A_STRUCT: &str = "E0329";

pub const NOT_A_STRUCT_LITERAL: &str = "E0330";

pub const DUPLICATE_FIELD_VALUE: &str = "E0331";

pub const MISSING_FIELDS: &str = "E0332";

pub const VARIANT_ENUM_UNKNOWN: &str = "E0333";

pub const VARIANT_BASE_NOT_A_TYPE: &str = "E0334";

pub const VARIANT_EXPR_PAYLOAD_SHAPE: &str = "E0335";

pub const RECORD_FIELD_UNKNOWN: &str = "E0336";

pub const VARIANT_MISSING_FIELDS: &str = "E0337";

pub const IF_NO_ELSE_MISMATCH: &str = "E0338";

pub const TRY_OPERAND_UNKNOWN: &str = "E0339";

pub const NOT_TRY: &str = "E0340";

pub const TRY_OUTSIDE: &str = "E0341";

pub const TRY_RETURN_MISMATCH: &str = "E0342";

pub const CAST_TARGET_NOT_PRIMITIVE: &str = "E0343";

pub const CAST_SOURCE_NOT_PRIMITIVE: &str = "E0344";

pub const CAST_OPERAND_UNKNOWN: &str = "E0345";

pub const CAST_NOT_ALLOWED: &str = "E0346";

pub const STRING_PATTERN_UNSUPPORTED: &str = "E0347";

pub const VARIANT_TYPE_UNKNOWN: &str = "E0348";

pub const NO_PAYLOAD_FIELD: &str = "E0349";

pub const MATCH_NEEDS_WILDCARD: &str = "E0350";

pub const REFUTABLE_LET_WITHOUT_ELSE: &str = "E0351";

pub const IRREFUTABLE_LET_WITH_ELSE: &str = "E0352";

pub const PATTERN_PAYLOAD_SHAPE: &str = "E0353";

pub const UNEXPECTED_GENERIC_ARGS: &str = "E0354";

pub const GENERIC_ARG_COUNT: &str = "E0355";

pub const TRAIT_AS_TYPE: &str = "E0356";

pub const UNSIZED_DYN: &str = "E0357";

pub const ARRAY_LEN_NOT_CONSTANT: &str = "E0358";

pub const ARRAY_LEN_NEGATIVE: &str = "E0359";

pub const ARRAY_LEN_OVERFLOW: &str = "E0360";

pub const ARRAY_LEN_DIVISION_BY_ZERO: &str = "E0361";

pub const REFERENCE_GENERIC_ARG: &str = "E0362";

pub const SELF_CYCLE: &str = "E0363";

pub const MISSING_MAIN: &str = "E0364";

pub const AMBIGUOUS_MAIN: &str = "E0365";

pub const MAIN_TAKES_PARAMETERS: &str = "E0366";

pub const MAIN_RETURNS_A_VALUE: &str = "E0367";

pub const MAIN_IS_GENERIC: &str = "E0368";

pub const NOT_MUTABLE: &str = "E0369";

pub const WRITE_THROUGH_SHARED_REF: &str = "E0370";

pub const RETURNED_LOCAL_REFERENCE: &str = "E0371";

pub const NO_METHOD: &str = "E0401";

pub const WRONG_ARG_COUNT: &str = "E0402";

pub const AMBIGUOUS_METHOD: &str = "E0403";

pub const NO_RECEIVER: &str = "E0404";

pub const RECEIVER_MODE: &str = "E0405";

pub const RECEIVER_NOT_A_PLACE: &str = "E0406";

pub const FIELD_IS_A_METHOD: &str = "E0407";

pub const NOT_CALLABLE: &str = "E0408";

pub const RECEIVER_TYPE_UNKNOWN: &str = "E0409";

pub const DYN_SELF_BY_VALUE: &str = "E0410";

pub const DYN_METHOD_MENTIONS_SELF: &str = "E0411";

pub const CONFLICTING_EXTENDS: &str = "E0412";

pub const DUPLICATE_METHOD: &str = "E0413";

pub const EXTEND_TRAIT: &str = "E0414";

pub const EXTEND_GENERIC: &str = "E0415";

pub const EXTEND_ANY: &str = "E0416";

pub const EXTEND_DYN: &str = "E0417";

pub const EXTEND_UNSIZED: &str = "E0418";

pub const EXTEND_BARE_SELF: &str = "E0419";

pub const EXTEND_WITH_NON_TRAIT: &str = "E0420";

pub const MISSING_METHODS: &str = "E0421";

pub const NOT_A_MEMBER: &str = "E0422";

pub const GENERIC_COUNT: &str = "E0423";

pub const SELF_MODE: &str = "E0424";

pub const PARAM_COUNT: &str = "E0425";

pub const PARAM_TY: &str = "E0426";

pub const RET_TY: &str = "E0427";

pub const UNSATISFIED_BOUND: &str = "E0428";

pub const BOUNDS_ANNOTATIONS_NEEDED: &str = "E0429";

pub const OPERATOR_TRAIT_MISSING: &str = "E0430";

pub const BOUND_IS_NOT_A_TRAIT: &str = "E0431";

pub const ARG_COUNT_MISMATCH: &str = "E0432";

pub const USE_OF_MOVED_VALUE: &str = "E0501";

pub const MOVE_OUT_OF_ARRAY: &str = "E0502";

pub const EXCLUSIVITY_VIOLATION: &str = "E0503";

pub const CAPTURED_REFERENCE: &str = "E0504";

pub const MOVE_OUT_OF_ENVIRONMENT: &str = "E0505";

pub const NOT_ALL_PATHS_RETURN: &str = "E0506";

pub const NEVER_READ: &str = "W0501";

pub const MISSING_LANG_ITEM: &str = "E0601";
