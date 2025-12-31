#include "AST/TypeSystem/Type.hpp"

#include <cstdint>
#include <format>
#include <string>

namespace phi {

std::string BuiltinTy::toString() const {
  switch (getBuiltinKind()) {
  case Kind::i8:
    return "i8";
  case Kind::i16:
    return "i16";
  case Kind::i32:
    return "i32";
  case Kind::i64:
    return "i64";
  case Kind::u8:
    return "u8";
  case Kind::u16:
    return "u16";
  case Kind::u32:
    return "u32";
  case Kind::u64:
    return "u64";
  case Kind::f32:
    return "f32";
  case Kind::f64:
    return "f64";
  case Kind::String:
    return "string";
  case Kind::Char:
    return "char";
  case Kind::Bool:
    return "bool";
  case Kind::Range:
    return "range";
  case Kind::Null:
    return "null";
  case Kind::Error:
    return "!";
  }
}

std::string AdtTy::toString() const { return getId(); }

std::string TupleTy::toString() const {
  std::string Ret = "(";
  for (uint64_t i = 0; i < ElementTys.size(); i++) {
    Ret += ElementTys[i].toString();
    if (i < ElementTys.size() - 1)
      Ret += ", ";
    else
      Ret += ")";
  }
  return Ret;
}

std::string FunTy::toString() const {
  std::string Params = "(";
  for (uint64_t i = 0; i < ParamTys.size(); i++) {
    Params += ParamTys[i].toString();
    if (i < ParamTys.size() - 1)
      Params += ", ";
    else
      Params += ")";
  }
  return std::format("fun{} -> {}", Params, ReturnTy.toString());
}

std::string PtrTy::toString() const { return "*" + Pointee.toString(); }

std::string RefTy::toString() const { return "&" + Pointee.toString(); }

std::string VarTy::toString() const { return "T" + std::to_string(N); }

std::string ErrTy::toString() const { return "Error"; }

} // namespace phi
