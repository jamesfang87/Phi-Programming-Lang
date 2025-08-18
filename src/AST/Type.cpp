#include "AST/Type.hpp"
#include "Sema/HMTI/HMType.hpp"
#include <memory>

namespace phi {
/**
 * Checks if type is an integer primitive.
 */
bool isIntTy(const Type &Ty) {
  if (!Ty.isPrimitive())
    return false;

  const Type::PrimitiveKind prim = Ty.getPrimitiveType();
  return prim == Type::PrimitiveKind::I8Kind ||
         prim == Type::PrimitiveKind::I16Kind ||
         prim == Type::PrimitiveKind::I32Kind ||
         prim == Type::PrimitiveKind::I64Kind ||
         prim == Type::PrimitiveKind::U8Kind ||
         prim == Type::PrimitiveKind::U16Kind ||
         prim == Type::PrimitiveKind::U32Kind ||
         prim == Type::PrimitiveKind::U64Kind;
}

bool isSignedInt(const Type &Ty) {
  if (!Ty.isPrimitive())
    return false;

  const Type::PrimitiveKind prim = Ty.getPrimitiveType();
  return prim == Type::PrimitiveKind::I8Kind ||
         prim == Type::PrimitiveKind::I16Kind ||
         prim == Type::PrimitiveKind::I32Kind ||
         prim == Type::PrimitiveKind::I64Kind;
}

bool isUnsignedInt(const Type &Ty) { return (isIntTy(Ty) && !isSignedInt(Ty)); }

bool isFloat(const Type &Ty) {
  if (!Ty.isPrimitive())
    return false;

  const Type::PrimitiveKind prim = Ty.getPrimitiveType();
  return prim == Type::PrimitiveKind::F32Kind ||
         prim == Type::PrimitiveKind::F64Kind;
}

/**
 * Checks if type is numeric (integer or float).
 */
bool isNumTy(const Type &Ty) {
  if (!Ty.isPrimitive())
    return false;

  return isIntTy(Ty) || isFloat(Ty);
}

std::shared_ptr<Monotype> Type::toMonotype() const {
  // Map AST::Type::PrimitiveKind -> Monotype constructor names
  switch (Kind) {
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
    return Monotype::con(getCustomTypeName());
  }
}

} // namespace phi
