#include "AST/Type.hpp"

namespace phi {
/**
 * Checks if type is an integer primitive.
 */
bool isIntTy(const Type &type) {
  if (!type.isPrimitive())
    return false;

  const Type::Primitive prim = type.primitive_type();
  return prim == Type::Primitive::i8 || prim == Type::Primitive::i16 ||
         prim == Type::Primitive::i32 || prim == Type::Primitive::i64 ||
         prim == Type::Primitive::u8 || prim == Type::Primitive::u16 ||
         prim == Type::Primitive::u32 || prim == Type::Primitive::u64;
}

bool isSignedInt(const Type &type) {
  if (!type.isPrimitive())
    return false;

  const Type::Primitive prim = type.primitive_type();
  return prim == Type::Primitive::i8 || prim == Type::Primitive::i16 ||
         prim == Type::Primitive::i32 || prim == Type::Primitive::i64;
}

bool isUnsignedInt(const Type &type) {
  return (isIntTy(type) && !isSignedInt(type));
}

bool isFloat(const Type &type) {
  if (!type.isPrimitive())
    return false;

  const Type::Primitive prim = type.primitive_type();
  return prim == Type::Primitive::f32 || prim == Type::Primitive::f64;
}

/**
 * Checks if type is numeric (integer or float).
 */
bool isNumTy(const Type &type) {
  if (!type.isPrimitive())
    return false;

  return isIntTy(type) || isFloat(type);
}

} // namespace phi
