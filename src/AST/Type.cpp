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

bool is_signed_int(const Type &type) {
  if (!type.isPrimitive())
    return false;

  const Type::Primitive prim = type.primitive_type();
  return prim == Type::Primitive::i8 || prim == Type::Primitive::i16 ||
         prim == Type::Primitive::i32 || prim == Type::Primitive::i64;
}

bool is_unsigned_int(const Type &type) {
  return (isIntTy(type) && !is_signed_int(type));
}

bool is_float_type(const Type &type) {
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

  return isIntTy(type) || is_float_type(type);
}

} // namespace phi
