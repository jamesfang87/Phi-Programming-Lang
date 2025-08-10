#include "AST/Type.hpp"

namespace phi {
/**
 * Checks if type is an integer primitive.
 */
bool isIntTy(const Type &Ty) {
  if (!Ty.isPrimitive())
    return false;

  const Type::Primitive prim = Ty.getPrimitiveType();
  return prim == Type::Primitive::i8 || prim == Type::Primitive::i16 ||
         prim == Type::Primitive::i32 || prim == Type::Primitive::i64 ||
         prim == Type::Primitive::u8 || prim == Type::Primitive::u16 ||
         prim == Type::Primitive::u32 || prim == Type::Primitive::u64;
}

bool isSignedInt(const Type &Ty) {
  if (!Ty.isPrimitive())
    return false;

  const Type::Primitive prim = Ty.getPrimitiveType();
  return prim == Type::Primitive::i8 || prim == Type::Primitive::i16 ||
         prim == Type::Primitive::i32 || prim == Type::Primitive::i64;
}

bool isUnsignedInt(const Type &Ty) { return (isIntTy(Ty) && !isSignedInt(Ty)); }

bool isFloat(const Type &Ty) {
  if (!Ty.isPrimitive())
    return false;

  const Type::Primitive prim = Ty.getPrimitiveType();
  return prim == Type::Primitive::f32 || prim == Type::Primitive::f64;
}

/**
 * Checks if type is numeric (integer or float).
 */
bool isNumTy(const Type &Ty) {
  if (!Ty.isPrimitive())
    return false;

  return isIntTy(Ty) || isFloat(Ty);
}

} // namespace phi
