#include "Sema/HMTI/Adapters/TypeAdapters.hpp"
#include "AST/Type.hpp"
#include <stdexcept>

namespace phi {

using namespace ::phi; // AST Type lives in phi

static std::shared_ptr<Monotype> fromBuiltin(const Type &T) {
  // Map AST::Type::PrimitiveKind -> Monotype constructor names
  switch (T.getPrimitiveType()) {
  case Type::PrimitiveKind::I8Kind:
    return Monotype::con("i8");
  case Type::PrimitiveKind::I16Kind:
    return Monotype::con("i16");
  case Type::PrimitiveKind::I32Kind:
    return Monotype::con("i32");
  case Type::PrimitiveKind::I64Kind:
    return Monotype::con("i64");
  case Type::PrimitiveKind::U8Kind:
    return Monotype::con("u8");
  case Type::PrimitiveKind::U16Kind:
    return Monotype::con("u16");
  case Type::PrimitiveKind::U32Kind:
    return Monotype::con("u32");
  case Type::PrimitiveKind::U64Kind:
    return Monotype::con("u64");
  case Type::PrimitiveKind::F32Kind:
    return Monotype::con("f32");
  case Type::PrimitiveKind::F64Kind:
    return Monotype::con("f64");
  case Type::PrimitiveKind::StringKind:
    return Monotype::con("string");
  case Type::PrimitiveKind::CharKind:
    return Monotype::con("char");
  case Type::PrimitiveKind::BoolKind:
    return Monotype::con("bool");
  case Type::PrimitiveKind::RangeKind:
    return Monotype::con("range");
  case Type::PrimitiveKind::NullKind:
    return Monotype::con("null");
  case Type::PrimitiveKind::CustomKind:
  default:
    throw std::runtime_error(
        "fromBuiltin: unexpected mapping; use fromAstType for custom types");
  }
}

std::shared_ptr<Monotype> fromAstType(const Type &T) {
  // Your AST Type is a single class that can be primitive or custom (CustomKind
  // uses stored name).
  if (!T.isPrimitive()) {
    // custom type name
    return Monotype::con(T.getCustomTypeName());
  }
  // It's primitive: map according to PrimitiveKind
  return fromBuiltin(T);
}

Type toAstType(const std::shared_ptr<Monotype> &T) {
  // AST.Type doesn't have function type; it's only primitive/custom.
  // For Var (still free) we default to i32 (safe conservative default).
  if (T->tag() == Monotype::Kind::Var) {
    return Type(Type::PrimitiveKind::I32Kind);
  }
  if (T->tag() == Monotype::Kind::Con) {
    const auto &Name = T->getConName();
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
  if (T->tag() == Monotype::Kind::Fun) {
    return toAstType(T->getFunReturn());
  }
  return Type(Type::PrimitiveKind::I32Kind);
}

} // namespace phi
