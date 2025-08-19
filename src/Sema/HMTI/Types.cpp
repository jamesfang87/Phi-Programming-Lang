#include "Sema/HMTI/TypeEnv.hpp"

#include <string>

namespace phi {

Type Monotype::toAstType() const {
  // AST.Type doesn't have function type; it's only primitive/custom.
  if (isVar()) {
    return Type("");
  }

  if (isCon()) {
    const auto &Name = asCon().name;
    if (Name == "i8")
      return Type(Type::PrimitiveKind::I8Kind);
    if (Name == "i16")
      return Type(Type::PrimitiveKind::I16Kind);
    if (Name == "i32")
      return Type(Type::PrimitiveKind::I32Kind);
    if (Name == "i64")
      return Type(Type::PrimitiveKind::I64Kind);
    if (Name == "u8")
      return Type(Type::PrimitiveKind::U8Kind);
    if (Name == "u16")
      return Type(Type::PrimitiveKind::U16Kind);
    if (Name == "u32")
      return Type(Type::PrimitiveKind::U32Kind);
    if (Name == "u64")
      return Type(Type::PrimitiveKind::U64Kind);
    if (Name == "f32")
      return Type(Type::PrimitiveKind::F32Kind);
    if (Name == "f64")
      return Type(Type::PrimitiveKind::F64Kind);
    if (Name == "string")
      return Type(Type::PrimitiveKind::StringKind);
    if (Name == "char")
      return Type(Type::PrimitiveKind::CharKind);
    if (Name == "bool")
      return Type(Type::PrimitiveKind::BoolKind);
    if (Name == "range")
      return Type(Type::PrimitiveKind::RangeKind);
    if (Name == "null")
      return Type(Type::PrimitiveKind::NullKind);
    // otherwise treat as custom/struct name
    return Type(Name);
  }
  // Fun: map to return type only — callers (Infer::annotate) handle
  // params+return
  if (isFun()) {
    return asFun().ret->toAstType();
  }
  return Type(Type::PrimitiveKind::I32Kind);
}

} // namespace phi
