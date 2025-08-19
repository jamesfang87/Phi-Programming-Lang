#include "AST/Type.hpp"
#include "Sema/HMTI/Types/Monotype.hpp"

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

Monotype Type::toMonotype() const {
  // Map AST::Type::PrimitiveKind -> Monotype constructor names
  switch (Kind) {
  case Type::PrimitiveKind::I8Kind:
    return Monotype::makeCon("i8");
  case Type::PrimitiveKind::I16Kind:
    return Monotype::makeCon("i16");
  case Type::PrimitiveKind::I32Kind:
    return Monotype::makeCon("i32");
  case Type::PrimitiveKind::I64Kind:
    return Monotype::makeCon("i64");
  case Type::PrimitiveKind::U8Kind:
    return Monotype::makeCon("u8");
  case Type::PrimitiveKind::U16Kind:
    return Monotype::makeCon("u16");
  case Type::PrimitiveKind::U32Kind:
    return Monotype::makeCon("u32");
  case Type::PrimitiveKind::U64Kind:
    return Monotype::makeCon("u64");
  case Type::PrimitiveKind::F32Kind:
    return Monotype::makeCon("f32");
  case Type::PrimitiveKind::F64Kind:
    return Monotype::makeCon("f64");
  case Type::PrimitiveKind::StringKind:
    return Monotype::makeCon("string");
  case Type::PrimitiveKind::CharKind:
    return Monotype::makeCon("char");
  case Type::PrimitiveKind::BoolKind:
    return Monotype::makeCon("bool");
  case Type::PrimitiveKind::RangeKind:
    return Monotype::makeCon("range");
  case Type::PrimitiveKind::NullKind:
    return Monotype::makeCon("null");
  case Type::PrimitiveKind::CustomKind:
    return Monotype::makeCon(getCustomTypeName());
  }
}

} // namespace phi
